//! Runtime acquisition of the sidecar binary + GGUF models — Rust-only, no Python
//! (`01 §5`; the user opted into runtime auto-download).
//!
//! - The **binary**: the latest `llama.cpp` Windows x64 **Vulkan** release zip from
//!   GitHub, extracted under app-data (or overridden via `SSV2C_LLAMA_RELEASE_URL`).
//! - The **models**: the tier's GGUF weights (preferring `Q4_K_M`) plus, for vision,
//!   the **same-repo** `mmproj` projector, pulled with `hf-hub` into the clean
//!   app-data layout (`MODEL_REGISTRY §4`).
//!
//! The selection logic (`pick_vulkan_asset`, `pick_gguf_files`) is pure and
//! unit-tested; the network fetches run at app first-run and in the gated manual
//! smoke (they are not exercised by `cargo test`).

use std::os::windows::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::stream::{self, StreamExt};
use hf_hub::api::tokio::{ApiBuilder, ApiError, ApiRepo, Progress};
use hf_hub::{Cache, Repo, RepoType};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use traits::{ModelDownloadPhase, ModelDownloadStatus};

use crate::models::{self, ModelLane, ModelTier};

/// How often the downloader samples real downloaded-byte progress (for the UI) and
/// re-checks for a stall while a fetch runs.
const PROGRESS_POLL: Duration = Duration::from_millis(750);

/// If no new bytes land for this long the download is treated as stalled and aborted.
/// hf-hub 0.4.3 builds its own `reqwest` client with **no** socket timeout and we run it
/// with no retries, so a dead CDN connection would otherwise block `bytes_stream().next()`
/// forever (the exact "download hanging" the partial 8B fetch hit). The partial is left on
/// disk, so the next attempt resumes where this one stopped. The local clean-layout copy of a
/// multi-GB blob (during which no new bytes are streamed) is *not* counted against this
/// window — the watchdog pauses while the `copying` flag is set (see `run_download`).
const STALL_TIMEOUT: Duration = Duration::from_secs(180);

/// Timeout for small JSON API probes (the HF tree-API size lookup, the GitHub releases
/// query). These run *before* any stall watchdog, so without a per-request timeout a hung
/// or slow endpoint would block the whole flow from starting (the watchdog can't rescue
/// it yet). Short and bounded — the responses are tiny and the work proceeds either way.
const HTTP_API_TIMEOUT: Duration = Duration::from_secs(15);

/// Connect timeout for the (large) binary download. A total timeout would wrongly abort a
/// legitimately slow multi-MB transfer, so we only guard the *connect* phase — enough to
/// fail fast on a dead host without capping a download that is actually making progress.
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-read-chunk timeout for the binary download body. Unlike a wall-clock `timeout`, this
/// fails a connection that goes *silent* mid-transfer (CDN accepts the socket then stalls)
/// without aborting a slow-but-progressing download — the binary path has no separate stall
/// watchdog, so `resp.bytes().await` would otherwise hang `init_inference` forever.
const HTTP_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// hf-hub guards each cached blob with a per-file **advisory** lock and gives up after a few
/// seconds with [`ApiError::LockAcquisition`] when another *live* downloader holds it. The app
/// enforces single-instance (so this is rare) and serializes per lane in-process, but a brief
/// old-instance→new-instance startup overlap could still race a fetch. Rather than surface a
/// hard error — which the vision scheduler then retries in a tight loop (the observed "download
/// storm") — we wait for the holder to release and re-attempt with linear backoff. A lock
/// failure streams no bytes, so retrying never double-counts progress. (A lock left by a *dead*
/// process is released by the OS, so this only ever waits on a genuinely live holder.)
const LOCK_RETRY_BACKOFF: Duration = Duration::from_secs(2);
/// Per-attempt backoff is capped here so the wait grows then plateaus instead of ballooning; with
/// [`LOCK_RETRY_MAX_ATTEMPTS`] the total patience is ~5 min — long enough to outlast a real
/// multi-GB download by the lock holder (the old ~20 s gave up far too early), while the
/// post-backoff cache re-check in [`fetch_one`] exits the instant the holder finishes, so the full
/// wait is rarely paid.
const LOCK_RETRY_BACKOFF_CAP: Duration = Duration::from_secs(15);
const LOCK_RETRY_MAX_ATTEMPTS: u32 = 24;

/// Parallel HTTP `Range` connections the chunked model downloader opens. HF's CDN throttles a
/// single connection to ~20 MB/s but serves many in parallel, so several connections saturate a
/// fast link; past ~8 the per-connection gain flattens while TLS/connection overhead grows.
/// Overridable via `SSV2C_DOWNLOAD_CONNECTIONS` (clamped to [`DOWNLOAD_CONNECTIONS_MIN`]..=
/// [`DOWNLOAD_CONNECTIONS_MAX`]); a range-less server downloads single-stream regardless.
const DOWNLOAD_CONNECTIONS: usize = 8;
const DOWNLOAD_CONNECTIONS_MIN: usize = 1;
const DOWNLOAD_CONNECTIONS_MAX: usize = 16;

/// Size of each `Range` request. Big enough that per-request HTTP/TLS overhead is negligible
/// against a multi-GB model, small enough that a mid-chunk interruption only re-fetches a
/// bounded amount on resume (≤ `chunk_size` × connections). Overridable via
/// `SSV2C_DOWNLOAD_CHUNK_SIZE` (bytes; floored at [`DOWNLOAD_CHUNK_SIZE_MIN`]).
const DOWNLOAD_CHUNK_SIZE: u64 = 32 * 1024 * 1024;
const DOWNLOAD_CHUNK_SIZE_MIN: u64 = 1024 * 1024;

/// Connect / per-read timeouts for the chunked downloader's HTTP client. The read timeout
/// fails a connection that goes *silent* mid-transfer — so one dead chunk socket fails fast
/// instead of pinning the pool — without aborting a slow-but-progressing one. The aggregate
/// stall watchdog in [`ModelDownloader::run_download`] is the backstop for "every chunk dead at
/// once" (no aggregate progress), unchanged by the chunked path since all chunks feed one
/// counter.
const CHUNK_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const CHUNK_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Bounded retry for a chunk request that comes back with a transient failure (a `403`/`401` from
/// a single-use signed CDN URL race, a `429`, or a `5xx`). Each retry re-requests the **resolve**
/// URL, so the redirect mints a fresh signed CDN URL — the same per-request resolution `hf` /
/// `hf_transfer` rely on. A chunk that exhausts these still leaves its `.part` for resume.
const CHUNK_RETRY_MAX_ATTEMPTS: u32 = 4;
const CHUNK_RETRY_BACKOFF: Duration = Duration::from_millis(400);

/// Coalesce streamed body frames up to this many bytes before handing them to a `spawn_blocking`
/// positioned write. reqwest yields frames as small as a single TCP segment (~16–64 KiB); spawning
/// a blocking task per frame churns the pool. Buffering to 256 KiB cuts the write-task count by an
/// order of magnitude while keeping per-chunk memory bounded (one buffer per in-flight connection).
const CHUNK_WRITE_BUFFER: usize = 256 * 1024;

/// GitHub "list releases" endpoint for the upstream llama.cpp project, newest first.
/// We scan the recent page rather than `/releases/latest`: llama.cpp's CI sometimes
/// publishes a release with an incomplete asset set (no `win-vulkan-x64` zip), and a
/// single-`latest` lookup would then fail outright instead of using the prior build.
const GITHUB_RELEASES: &str =
    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10";
/// GitHub requires a User-Agent on API requests.
const USER_AGENT: &str = "screensearch-v2c";

/// Last-resort Windows x64 Vulkan asset used **only** when the anonymous releases lookup
/// fails outright. The API quota is per-IP and only 60 requests an hour: a shared/NAT'd
/// address that a user does not control can exhaust it, and before this fallback existed a
/// single 403 left the app with no sidecar for the whole session (observed in the field:
/// `llama.cpp release query returned an error status`, empty `sidecar/` dir, no GGUF).
/// The resolved asset is the same artifact the API would have selected — a GitHub
/// *release-asset* download, served from a different host that is not subject to the API
/// quota — so only the resolution source is pinned, not the download path. The normal
/// recent-release lookup still wins whenever it works, and `SSV2C_LLAMA_RELEASE_URL`
/// still wins over both.
const FALLBACK_VULKAN_ASSET: (&str, &str) = (
    "https://github.com/ggml-org/llama.cpp/releases/download/b11739/llama-b11739-bin-win-vulkan-x64.zip",
    "llama-b11739-bin-win-vulkan-x64.zip",
);
/// SHA-256 of [`FALLBACK_VULKAN_ASSET`] as reported by the GitHub release metadata. A
/// pinned artifact must be checked before it is run. GitHub's release JSON does expose a
/// per-asset `digest`, but this resolver does not parse it yet, so verification covers the
/// pin — the one download whose digest this code owns (`07` #96 tracks the API path).
const FALLBACK_VULKAN_SHA256: &str =
    "2a26916f2db1955b6c836476c60627604cd1d2b04533eacad0c76328c9aba249";

/// Bounded retry for the one binary zip fetch. GitHub's release-asset host can answer a
/// single-use signed-CDN request with `401`/`403`, or a transient `429`/`5xx`; the zip is
/// small (~32 MB) and fully re-fetchable, so a straight retry costs far less than a
/// session with no sidecar. Mirrors the chunked model downloader's retry policy.
const BINARY_RETRY_MAX_ATTEMPTS: u32 = 3;
const BINARY_RETRY_BACKOFF: Duration = Duration::from_millis(400);

#[derive(Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Picks the Windows x64 Vulkan release asset (`*-bin-win-vulkan-x64.zip`) from a list
/// of asset names. Returns its index, or `None` if the release has no such asset.
pub fn pick_vulkan_asset(names: &[String]) -> Option<usize> {
    names.iter().position(|n| {
        let n = n.to_ascii_lowercase();
        n.ends_with(".zip") && n.contains("win") && n.contains("vulkan") && n.contains("x64")
    })
}

/// From a newest-first list of releases, returns the `(download_url, asset_name)` of the
/// Windows x64 Vulkan zip in the most recent release that actually carries one. Some
/// llama.cpp releases ship an incomplete asset set, so the newest release is not always
/// usable — we take the newest that is. Returns `None` if no recent release has the asset.
fn pick_vulkan_from_releases(releases: &[GithubRelease]) -> Option<(String, String)> {
    releases.iter().find_map(|r| {
        let names: Vec<String> = r.assets.iter().map(|a| a.name.clone()).collect();
        let idx = pick_vulkan_asset(&names)?;
        let asset = &r.assets[idx];
        Some((asset.browser_download_url.clone(), asset.name.clone()))
    })
}

/// From a repo's file list, picks the weights GGUF (preferring a `Q4_K_M` quant,
/// excluding the projector) and — when `needs_mmproj` — the `mmproj*.gguf` projector.
/// Both are chosen deterministically (sorted) so re-resolution is stable.
pub fn pick_gguf_files(
    filenames: &[String],
    needs_mmproj: bool,
) -> (Option<String>, Option<String>) {
    let is_gguf = |n: &str| n.to_ascii_lowercase().ends_with(".gguf");
    let is_mmproj = |n: &str| {
        Path::new(n)
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.to_ascii_lowercase().starts_with("mmproj"))
    };

    let mut weights: Vec<String> = filenames
        .iter()
        .filter(|n| is_gguf(n) && !is_mmproj(n))
        .cloned()
        .collect();
    weights.sort();
    let gguf = weights
        .iter()
        .find(|n| n.to_ascii_lowercase().contains("q4_k_m"))
        .cloned()
        .or_else(|| weights.first().cloned());

    let mmproj = if needs_mmproj {
        let mut mp: Vec<String> = filenames
            .iter()
            .filter(|n| is_gguf(n) && is_mmproj(n))
            .cloned()
            .collect();
        mp.sort();
        mp.into_iter().next()
    } else {
        None
    };

    (gguf, mmproj)
}

/// Ensures `llama-server.exe` exists under `sidecar_dir`, downloading + extracting the
/// prebuilt Vulkan release if absent. Returns the path to the binary. Idempotent.
/// `SSV2C_LLAMA_RELEASE_URL` deliberately wins over a normal existing install and
/// lands in a URL-specific override directory, so testers can pin a sidecar build
/// without deleting the app-managed release.
pub async fn ensure_binary(sidecar_dir: &Path) -> Result<PathBuf> {
    if let Some((url, name)) = env_binary_override() {
        let install = sidecar_dir
            .join("llama-override")
            .join(url_fingerprint(&url));
        // An operator override is trusted, but when it *is* the pin we know its digest, so
        // verify it like the managed fallback rather than skipping the check.
        return ensure_binary_from_url_verified(&install, &url, &name, fallback_sha256(&url)).await;
    }

    let install = sidecar_dir.join("llama");
    if let Some(found) = find_file(&install, "llama-server.exe") {
        return Ok(found);
    }

    let (url, name) = resolve_binary_url().await;
    ensure_binary_from_url_verified(&install, &url, &name, fallback_sha256(&url)).await
}

/// The pinned fallback asset is the only download with a known-good digest; the API path
/// has none in our release JSON, so it stays unverified rather than gaining a fake gate.
fn fallback_sha256(url: &str) -> Option<&'static str> {
    (url == FALLBACK_VULKAN_ASSET.0).then_some(FALLBACK_VULKAN_SHA256)
}

async fn ensure_binary_from_url_verified(
    install: &Path,
    url: &str,
    name: &str,
    expected_sha256: Option<&str>,
) -> Result<PathBuf> {
    if let Some(found) = find_file(install, "llama-server.exe") {
        return Ok(found);
    }

    tracing::info!(asset = %name, "downloading llama.cpp Vulkan release");
    let bytes = http_get_bytes(url).await?;
    if let Some(expected) = expected_sha256 {
        verify_sha256(&bytes, expected).with_context(|| format!("verify sidecar asset {name}"))?;
    }
    let install = install.to_path_buf();
    let partial = partial_install_dir(&install);
    let install_for_task = install.clone();
    let partial_for_task = partial.clone();
    let partial_for_cleanup = partial.clone();
    let install_result = tokio::task::spawn_blocking(move || {
        if let Some(parent) = install_for_task.parent() {
            std::fs::create_dir_all(parent).context("create sidecar install parent dir")?;
        }
        if partial_for_task.exists() {
            std::fs::remove_dir_all(&partial_for_task).with_context(|| {
                format!(
                    "remove stale partial install {}",
                    partial_for_task.display()
                )
            })?;
        }
        std::fs::create_dir_all(&partial_for_task).context("create sidecar partial install dir")?;
        extract_zip(&bytes, &partial_for_task).context("extract llama release zip")?;
        if find_file(&install_for_task, "llama-server.exe").is_some() {
            std::fs::remove_dir_all(&partial_for_task).with_context(|| {
                format!(
                    "remove redundant partial install {}",
                    partial_for_task.display()
                )
            })?;
            return Ok(());
        }
        if install_for_task.exists() {
            std::fs::remove_dir_all(&install_for_task).with_context(|| {
                format!(
                    "remove incomplete sidecar install dir {}",
                    install_for_task.display()
                )
            })?;
        }
        std::fs::rename(&partial_for_task, &install_for_task).with_context(|| {
            format!(
                "finalize sidecar install {} -> {}",
                partial_for_task.display(),
                install_for_task.display()
            )
        })?;
        Ok(())
    })
    .await
    .context("sidecar binary install task failed")?;
    if let Err(err) = install_result {
        cleanup_partial_install(partial_for_cleanup).await;
        return Err(err);
    }

    find_file(&install, "llama-server.exe")
        .with_context(|| format!("llama-server.exe not found in release asset {name}"))
}

/// Exact installed `llama-server.exe` paths this app owns under the normal and
/// URL-specific override sidecar install roots. Used as startup reap sentinels.
pub fn installed_binary_candidates(sidecar_dir: &Path) -> Vec<PathBuf> {
    let mut binaries = Vec::new();
    collect_files_named(
        &sidecar_dir.join("llama"),
        "llama-server.exe",
        &mut binaries,
    );
    collect_files_named(
        &sidecar_dir.join("llama-override"),
        "llama-server.exe",
        &mut binaries,
    );
    binaries.sort_by_key(|p| p.to_string_lossy().to_ascii_lowercase());
    binaries.dedup_by(|a, b| {
        a.to_string_lossy()
            .as_ref()
            .eq_ignore_ascii_case(b.to_string_lossy().as_ref())
    });
    binaries
}

/// Ensures the `(lane, tier)` model files are present under the app-data models dir,
/// downloading the weights (+ same-repo projector for vision) if missing. Idempotent —
/// a file already present is left as-is.
pub async fn ensure_model(models_root: &Path, lane: ModelLane, tier: ModelTier) -> Result<()> {
    let dir = models::local_dir(models_root, lane, tier);
    std::fs::create_dir_all(&dir).context("create model dir")?;

    let repo = models::repo_for(lane, tier);
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(models_root.join(".hf-cache"))
        .build()
        .context("build hf-hub api")?;
    let repo_api = api.model(repo.repo_id.to_string());

    let info = repo_api
        .info()
        .await
        .with_context(|| format!("list files in {}", repo.repo_id))?;
    let names: Vec<String> = info.siblings.into_iter().map(|s| s.rfilename).collect();
    let (gguf, mmproj) = pick_gguf_files(&names, repo.needs_mmproj);

    let gguf = gguf.with_context(|| format!("no GGUF weights found in {}", repo.repo_id))?;
    download_into(&repo_api, &gguf, &dir).await?;

    if repo.needs_mmproj {
        let mmproj =
            mmproj.with_context(|| format!("no mmproj projector found in {}", repo.repo_id))?;
        download_into(&repo_api, &mmproj, &dir).await?;
    }
    Ok(())
}

/// The base filename (no directory) a repo file takes in our clean layout.
fn clean_base(filename: &str) -> &str {
    Path::new(filename)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(filename)
}

/// Copies a cached blob into our clean `<models_root>/<lane>/<tier>` layout so
/// [`crate::models::resolve_spec`] can find it by scanning the dir. Atomic: write to a temp
/// file in the same dir, then rename. An interrupted copy must never leave a partial file at
/// `dest` — a later `dest.exists()` skip would otherwise treat a corrupt GGUF as complete
/// and crash the sidecar.
fn place_in_clean_layout(cached: &Path, dir: &Path, base: &str) -> Result<()> {
    let dest = dir.join(base);
    let tmp = dir.join(format!("{base}.partial"));
    std::fs::copy(cached, &tmp).with_context(|| format!("copy {base} into {}", dir.display()))?;
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("finalize {}", dest.display()));
    }
    Ok(())
}

/// [`place_in_clean_layout`] off the async runtime — copying a multi-GB blob would otherwise
/// block a worker thread for seconds (starving the event bridge and other requests).
async fn place_in_clean_layout_async(cached: PathBuf, dir: &Path, base: &str) -> Result<()> {
    let dir = dir.to_path_buf();
    let base = base.to_string();
    tokio::task::spawn_blocking(move || place_in_clean_layout(&cached, &dir, &base))
        .await
        .context("clean-layout copy task failed")?
}

/// Downloads one repo file (via the HF cache) and copies it into our clean layout so
/// [`crate::models::resolve_spec`] can find it by scanning `dir`. Skips if present. No
/// progress reporting — used by the gated smoke tests; the app path uses [`ModelDownloader`].
async fn download_into(repo: &ApiRepo, filename: &str, dir: &Path) -> Result<()> {
    let base = clean_base(filename);
    if dir.join(base).exists() {
        return Ok(());
    }
    let cached = repo
        .get(filename)
        .await
        .with_context(|| format!("download {filename}"))?;
    place_in_clean_layout(&cached, dir, base)
}

async fn resolve_binary_url() -> (String, String) {
    resolve_binary_url_with_fallback(GITHUB_RELEASES).await
}

/// Resolves the sidecar asset from the releases API, falling back to the pinned
/// [`FALLBACK_VULKAN_ASSET`] when that API call fails for any reason (the quota is
/// per-IP, so an exhausted shared address is not recoverable by retrying here). The
/// API result always wins when it succeeds, so new builds are still picked up normally.
async fn resolve_binary_url_with_fallback(releases_url: &str) -> (String, String) {
    match resolve_binary_url_from(releases_url).await {
        Ok(asset) => asset,
        Err(e) => {
            tracing::warn!(
                error = %format!("{e:#}"),
                asset = FALLBACK_VULKAN_ASSET.1,
                "llama.cpp release lookup failed; using the pinned fallback sidecar asset"
            );
            (
                FALLBACK_VULKAN_ASSET.0.to_string(),
                FALLBACK_VULKAN_ASSET.1.to_string(),
            )
        }
    }
}

async fn resolve_binary_url_from(releases_url: &str) -> Result<(String, String)> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_API_TIMEOUT)
        .build()
        .context("build releases http client")?;
    let resp = client
        .get(releases_url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .context("query llama.cpp releases")?
        .error_for_status()
        .context("llama.cpp release query returned an error status")?;
    let releases: Vec<GithubRelease> = resp.json().await.context("decode releases json")?;
    pick_vulkan_from_releases(&releases)
        .context("no win-vulkan-x64 asset in any recent llama.cpp release")
}

fn env_binary_override() -> Option<(String, String)> {
    let url = std::env::var("SSV2C_LLAMA_RELEASE_URL").ok()?;
    let name = url.rsplit('/').next().unwrap_or("llama.zip").to_string();
    Some((url, name))
}

fn url_fingerprint(url: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in url.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn partial_install_dir(install: &Path) -> PathBuf {
    let name = install
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("llama");
    install.with_file_name(format!("{name}.partial"))
}

async fn cleanup_partial_install(partial: PathBuf) {
    let _ = tokio::task::spawn_blocking(move || {
        if partial.exists() {
            let _ = std::fs::remove_dir_all(partial);
        }
    })
    .await;
}

async fn http_get_bytes(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .read_timeout(HTTP_READ_TIMEOUT)
        .build()
        .context("build download http client")?;
    let mut attempt = 1;
    loop {
        match fetch_binary_body(&client, url).await {
            Ok(bytes) => return Ok(bytes),
            Err(failure) if failure.retryable && attempt < BINARY_RETRY_MAX_ATTEMPTS => {
                tracing::warn!(
                    attempt,
                    error = %format!("{:#}", failure.error),
                    "sidecar asset download failed; retrying"
                );
                tokio::time::sleep(BINARY_RETRY_BACKOFF * attempt).await;
                attempt += 1;
            }
            Err(failure) => return Err(failure.error),
        }
    }
}

/// One attempt at the binary download. `retryable` marks the failures that a fresh
/// connection can plausibly fix (transport errors, a single-use signed-CDN `401`/`403`,
/// `408`/`429`, and `5xx`) — never a URL that simply does not exist.
struct BinaryFetchError {
    error: anyhow::Error,
    retryable: bool,
}

async fn fetch_binary_body(
    client: &reqwest::Client,
    url: &str,
) -> std::result::Result<Vec<u8>, BinaryFetchError> {
    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| BinaryFetchError {
            error: anyhow!(e).context(format!("download {url}")),
            retryable: true,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        return Err(BinaryFetchError {
            error: anyhow!("download {url} returned HTTP {status}"),
            retryable: code == 401
                || code == 403
                || code == 408
                || code == 429
                || status.is_server_error(),
        });
    }
    let bytes = resp.bytes().await.map_err(|e| BinaryFetchError {
        error: anyhow!(e).context(format!("read download body for {url}")),
        retryable: true,
    })?;
    Ok(bytes.to_vec())
}

/// Constant-time-insensitive digest check for a pinned asset: the file is downloaded over
/// TLS from GitHub and then compared with the digest published in the release metadata.
fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<()> {
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(anyhow!(
            "sha256 mismatch: expected {expected_hex}, got {actual}"
        ))
    }
}

/// Extracts a zip archive into `dest`, preserving its internal structure.
fn extract_zip(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("open zip archive")?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // skip unsafe / absolute paths
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    Ok(())
}

/// Recursively searches `dir` for a file named `name` (case-insensitive).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            return Some(p);
        }
    }
    None
}

fn collect_files_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_named(&path, name, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            out.push(path);
        }
    }
}

/// Coordinates model downloads: serializes them per lane (so concurrent enrichment
/// workers never race the same multi-GB fetch — the race that dead-lettered the quality
/// vision jobs) and broadcasts [`ModelDownloadStatus`] so the UI can show progress and
/// surface errors instead of opaque network activity (`03 §6/§7`). Construct one at the
/// composition root; share it with both lane providers and bridge [`Self::subscribe`].
pub struct ModelDownloader {
    models_root: PathBuf,
    events: broadcast::Sender<ModelDownloadStatus>,
    /// Per-lane download mutex. Index by lane: Vision = 0, Answer = 1.
    locks: [AsyncMutex<()>; 2],
}

impl ModelDownloader {
    pub fn new(models_root: PathBuf) -> Arc<Self> {
        let (events, _rx) = broadcast::channel(64);
        Arc::new(Self {
            models_root,
            events,
            locks: [AsyncMutex::new(()), AsyncMutex::new(())],
        })
    }

    /// Subscribe to download progress (bridged to the `model_download` UI event).
    pub fn subscribe(&self) -> broadcast::Receiver<ModelDownloadStatus> {
        self.events.subscribe()
    }

    fn lock_for(&self, lane: ModelLane) -> &AsyncMutex<()> {
        match lane {
            ModelLane::Vision => &self.locks[0],
            ModelLane::Answer => &self.locks[1],
        }
    }

    /// Ensures the `(lane, tier)` model files are on disk, downloading them with progress
    /// events if needed. Idempotent and serialized per lane: a second caller for the same
    /// lane waits for the first to finish, then sees the files present (no double fetch).
    pub async fn ensure(&self, lane: ModelLane, tier: ModelTier) -> Result<()> {
        if models::model_files_present(&self.models_root, lane, tier) {
            return Ok(());
        }
        let _guard = self.lock_for(lane).lock().await;
        // Re-check under the lock: another task may have just completed the download.
        if models::model_files_present(&self.models_root, lane, tier) {
            return Ok(());
        }
        self.run_download(lane, tier).await
    }

    /// Downloads the `(lane, tier)` files, reporting **real** downloaded-byte progress and
    /// aborting a stalled fetch instead of hanging forever.
    ///
    /// Progress is driven by hf-hub's [`Progress`] callbacks (true streamed bytes, resume
    /// aware), not the on-disk file size: hf-hub pre-allocates the `.sync.part` to the full
    /// length up front (`set_len`), so a size poll jumps to ~100% immediately and masks both
    /// real progress *and* a stall. A watchdog reads the live byte counter; if it stops
    /// advancing for [`STALL_TIMEOUT`] the download task is aborted and a retryable error is
    /// surfaced — the partial stays on disk, so the next `ensure` resumes from there.
    async fn run_download(&self, lane: ModelLane, tier: ModelTier) -> Result<()> {
        let repo = models::repo_for(lane, tier);
        let model_label = repo
            .repo_id
            .rsplit('/')
            .next()
            .unwrap_or(repo.repo_id)
            .to_string();
        let dir = models::local_dir(&self.models_root, lane, tier);
        std::fs::create_dir_all(&dir).context("create model dir")?;
        let cache_dir = self.models_root.join(".hf-cache");
        let total = total_download_bytes(repo.repo_id, repo.needs_mmproj).await;

        tracing::info!(repo = repo.repo_id, ?total, "model download started");
        self.emit(
            lane,
            &model_label,
            ModelDownloadPhase::Downloading,
            0,
            total,
            None,
        );

        // Fetch on a child task so the watchdog can abort it on a stall. `copying` flags the
        // local clean-layout copy phases (publishing a cached/just-downloaded blob), which are
        // disk I/O — not network — and can exceed the stall timeout on a slow disk for a
        // multi-GB file. The watchdog pauses its stall counter while it is set.
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let task = {
            let cache_dir = cache_dir.clone();
            let dir = dir.clone();
            let repo_id = repo.repo_id.to_string();
            let needs_mmproj = repo.needs_mmproj;
            let downloaded = downloaded.clone();
            let copying = copying.clone();
            tokio::spawn(async move {
                download_repo_files(
                    &cache_dir,
                    &repo_id,
                    needs_mmproj,
                    &dir,
                    downloaded,
                    copying,
                )
                .await
            })
        };

        let limit = stall_limit(STALL_TIMEOUT, PROGRESS_POLL);
        let mut last_seen = 0u64;
        let mut stale = 0u32;
        loop {
            tokio::time::sleep(PROGRESS_POLL).await;
            let d = downloaded.load(Ordering::Relaxed);
            if task.is_finished() {
                let joined = task.await;
                return self.finish_download(lane, &model_label, total, d, joined);
            }
            self.emit(
                lane,
                &model_label,
                ModelDownloadPhase::Downloading,
                d,
                total,
                None,
            );
            if copying.load(Ordering::Relaxed) {
                // A local clean-layout copy is in progress — disk I/O, not a network
                // transfer. Don't let the network stall watchdog fire on it (a multi-GB copy
                // on a slow disk can outlast the timeout). Hold the counter steady.
                last_seen = d;
                stale = 0;
                continue;
            }
            (last_seen, stale) = stall_step(d, last_seen, stale);
            if stale >= limit {
                task.abort();
                let msg = "download stalled — no data received; click Load to resume".to_string();
                tracing::warn!(
                    repo = repo.repo_id,
                    downloaded = d,
                    "model download stalled; aborting"
                );
                self.emit(
                    lane,
                    &model_label,
                    ModelDownloadPhase::Failed,
                    d,
                    total,
                    Some(msg.clone()),
                );
                return Err(anyhow!(msg));
            }
        }
    }

    /// Emits the terminal download status (Done / Failed) and maps the join result to a
    /// `Result`. `d` is the last real byte count the watchdog observed.
    fn finish_download(
        &self,
        lane: ModelLane,
        model_label: &str,
        total: Option<u64>,
        d: u64,
        joined: Result<Result<()>, tokio::task::JoinError>,
    ) -> Result<()> {
        match joined {
            Ok(Ok(())) => {
                tracing::info!(model = model_label, "model download finished");
                self.emit(
                    lane,
                    model_label,
                    ModelDownloadPhase::Done,
                    total.unwrap_or(d),
                    total,
                    None,
                );
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::warn!(model = model_label, error = %e, "model download failed");
                self.emit(
                    lane,
                    model_label,
                    ModelDownloadPhase::Failed,
                    d,
                    total,
                    Some(e.to_string()),
                );
                Err(e)
            }
            Err(join_err) => {
                let e = anyhow!("download task failed: {join_err}");
                tracing::warn!(model = model_label, error = %e, "model download task error");
                self.emit(
                    lane,
                    model_label,
                    ModelDownloadPhase::Failed,
                    d,
                    total,
                    Some(e.to_string()),
                );
                Err(e)
            }
        }
    }

    fn emit(
        &self,
        lane: ModelLane,
        model: &str,
        phase: ModelDownloadPhase,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        error: Option<String>,
    ) {
        let _ = self.events.send(ModelDownloadStatus {
            lane,
            model: Some(model.to_string()),
            phase,
            downloaded_bytes,
            total_bytes,
            error,
        });
    }
}

/// hf-hub [`Progress`] sink that accumulates real streamed bytes into a shared counter the
/// download watchdog reads. On resume hf-hub reports the already-present prefix via one
/// `update(start)` call, so the count is correct across restarts.
#[derive(Clone)]
struct ByteCounter {
    downloaded: Arc<AtomicU64>,
}

impl Progress for ByteCounter {
    async fn init(&mut self, _size: usize, _filename: &str) {}
    async fn update(&mut self, size: usize) {
        self.downloaded.fetch_add(size as u64, Ordering::Relaxed);
    }
    async fn finish(&mut self) {}
}

// PR8 (0.2.0): each file is fetched by the parallel chunked downloader in [`fetch_one`] — N HTTP
// `Range` requests writing into one pre-allocated `.part` — to saturate bandwidth instead of being
// capped at hf-hub's single throttled connection (~20 MB/s). A range-less server falls back to the
// single-stream hf-hub path. The progress/stall/clean-layout machinery is reused, not bypassed.
/// Resolves the repo's GGUF (+ same-repo mmproj for vision) and fetches each into the clean
/// layout, accumulating real downloaded bytes into `downloaded` for the progress watchdog.
async fn download_repo_files(
    cache_dir: &Path,
    repo_id: &str,
    needs_mmproj: bool,
    dir: &Path,
    downloaded: Arc<AtomicU64>,
    copying: Arc<AtomicBool>,
) -> Result<()> {
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(cache_dir.to_path_buf())
        .build()
        .context("build hf-hub api")?;
    let repo_api = api.model(repo_id.to_string());

    let info = repo_api
        .info()
        .await
        .with_context(|| format!("list files in {repo_id}"))?;
    let names: Vec<String> = info.siblings.into_iter().map(|s| s.rfilename).collect();
    let (gguf, mmproj) = pick_gguf_files(&names, needs_mmproj);

    let gguf = gguf.with_context(|| format!("no GGUF weights found in {repo_id}"))?;
    fetch_one(
        &repo_api,
        cache_dir,
        repo_id,
        &gguf,
        dir,
        &downloaded,
        &copying,
    )
    .await?;

    if needs_mmproj {
        let mmproj = mmproj.with_context(|| format!("no mmproj projector found in {repo_id}"))?;
        fetch_one(
            &repo_api,
            cache_dir,
            repo_id,
            &mmproj,
            dir,
            &downloaded,
            &copying,
        )
        .await?;
    }
    Ok(())
}

/// Fetches one repo file into the clean layout with real-byte progress. Checks in order:
/// already in the clean layout → already a finalized blob in the HF cache (no network) →
/// **parallel chunked download** (N `Range` requests, resumable) → single-stream hf-hub
/// fallback for a range-less server. The chunked path publishes its own `.part` into the
/// layout; the skip/fallback paths add the on-disk size or stream via hf-hub's [`Progress`]
/// callbacks so the bar reflects every byte (already-present or freshly fetched).
async fn fetch_one(
    repo_api: &ApiRepo,
    cache_dir: &Path,
    repo_id: &str,
    filename: &str,
    dir: &Path,
    downloaded: &Arc<AtomicU64>,
    copying: &Arc<AtomicBool>,
) -> Result<()> {
    let base = clean_base(filename);

    // Already in the clean layout, or a finalized blob waiting in the HF cache — no network.
    if place_if_cached(cache_dir, repo_id, filename, dir, base, downloaded, copying).await? {
        return Ok(());
    }

    // Parallel chunked path: probe the CDN; if it advertises ranged downloads, fetch the file
    // in N parallel connections into a pre-allocated `.part` (resumable across restarts) and
    // publish it. A chunked error propagates (the `.part` stays for resume) rather than falling
    // back — only a range-less/probe-failed server takes the single-stream path below.
    let cfg = ChunkConfig::from_env();
    let resolve_url = hf_resolve_url(repo_id, filename);
    if let Some(info) = probe_range(&resolve_url, &cfg).await {
        if range_plan(info.is_partial, info.accept_ranges, info.total) {
            return chunked_download(&info, &resolve_url, dir, base, downloaded, copying, &cfg)
                .await;
        }
    }

    // Single-stream fallback (range-less server, or probe failure): hf-hub path with a lock-aware
    // retry. hf-hub guards each blob with an advisory lock; when another *live* downloader holds it
    // we back off and retry rather than surfacing a hard error the vision scheduler then retries in
    // a tight loop (the observed "download storm"). A lock failure streams no bytes, so retrying
    // never double-counts. After each backoff we re-check the cache fast paths first: if the holder
    // finished during the sleep, copy its finalized blob into place instead of re-downloading it or
    // colliding on publish (PR #27 Codex-P2). Non-lock errors propagate immediately.
    let mut attempt: u32 = 0;
    loop {
        let progress = ByteCounter {
            downloaded: downloaded.clone(),
        };
        match repo_api.download_with_progress(filename, progress).await {
            Ok(cached) => {
                // The streamed bytes are already counted; the publish copy is local I/O, so flag it
                // to pause the stall watchdog rather than re-counting (which would push the bar past
                // 100%).
                copying.store(true, Ordering::Relaxed);
                let placed = place_in_clean_layout_async(cached, dir, base).await;
                copying.store(false, Ordering::Relaxed);
                return placed;
            }
            Err(e)
                if matches!(e, ApiError::LockAcquisition(_))
                    && attempt < LOCK_RETRY_MAX_ATTEMPTS =>
            {
                attempt += 1;
                let backoff = (LOCK_RETRY_BACKOFF * attempt).min(LOCK_RETRY_BACKOFF_CAP);
                tracing::warn!(
                    filename,
                    attempt,
                    backoff_secs = backoff.as_secs(),
                    "model file lock held by another download; backing off and retrying"
                );
                tokio::time::sleep(backoff).await;
                // The lock holder may have finished during the backoff — grab its finalized blob
                // and short-circuit rather than re-downloading or racing it on publish.
                if place_if_cached(cache_dir, repo_id, filename, dir, base, downloaded, copying)
                    .await?
                {
                    return Ok(());
                }
            }
            Err(e) => return Err(e).with_context(|| format!("download {filename}")),
        }
    }
}

/// Checks the two no-network fast paths for `filename` and, if either holds, makes the file present
/// in the clean layout: it is already in the clean layout, or a finalized blob sits in the HF cache
/// (a prior run — or another instance that just released the lock — downloaded it but never copied
/// it across), which we copy into place. Returns `Ok(true)` when the file is now available in the
/// clean layout (nothing left to fetch), `Ok(false)` when neither path applied. The cache copy is
/// local disk I/O, so it flags `copying` to pause the network stall watchdog; the file's on-disk
/// size is added to `downloaded` exactly once so the progress bar stays truthful.
async fn place_if_cached(
    cache_dir: &Path,
    repo_id: &str,
    filename: &str,
    dir: &Path,
    base: &str,
    downloaded: &Arc<AtomicU64>,
    copying: &Arc<AtomicBool>,
) -> Result<bool> {
    let dest = dir.join(base);
    if dest.exists() {
        if let Ok(meta) = std::fs::metadata(&dest) {
            downloaded.fetch_add(meta.len(), Ordering::Relaxed);
        }
        return Ok(true);
    }

    let cache = Cache::new(cache_dir.to_path_buf());
    if let Some(cached) = cache
        .repo(Repo::new(repo_id.to_string(), RepoType::Model))
        .get(filename)
    {
        let size = std::fs::metadata(&cached).map(|m| m.len()).unwrap_or(0);
        copying.store(true, Ordering::Relaxed);
        let placed = place_in_clean_layout_async(cached, dir, base).await;
        copying.store(false, Ordering::Relaxed);
        placed?;
        downloaded.fetch_add(size, Ordering::Relaxed);
        return Ok(true);
    }

    Ok(false)
}

// ── Parallel chunked downloader (PR8) ──────────────────────────────────────────────────────
//
// N parallel HTTP `Range` requests stream into one pre-allocated `.part`, with a per-chunk
// resume bitmap so an interrupted download continues where it stopped. All chunks feed the same
// `downloaded` counter, so the existing aggregate-progress stall watchdog and the UI percentage
// stay truthful with concurrent writers. On success the assembled file is verified (LFS sha256
// when advertised, else byte length) and atomically renamed into the clean layout.

/// Tuning for the chunked downloader. Production reads it from [`ChunkConfig::from_env`]
/// (constants + `SSV2C_*` overrides); tests pass tiny chunk sizes and millisecond timeouts so
/// resume / stall behaviour is exercised deterministically without real network waits.
#[derive(Debug, Clone, Copy)]
struct ChunkConfig {
    conns: usize,
    chunk_size: u64,
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl ChunkConfig {
    fn from_env() -> Self {
        let conns = std::env::var("SSV2C_DOWNLOAD_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DOWNLOAD_CONNECTIONS)
            .clamp(DOWNLOAD_CONNECTIONS_MIN, DOWNLOAD_CONNECTIONS_MAX);
        let chunk_size = std::env::var("SSV2C_DOWNLOAD_CHUNK_SIZE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DOWNLOAD_CHUNK_SIZE)
            .max(DOWNLOAD_CHUNK_SIZE_MIN);
        Self {
            conns,
            chunk_size,
            connect_timeout: CHUNK_CONNECT_TIMEOUT,
            read_timeout: CHUNK_READ_TIMEOUT,
        }
    }
}

/// The HuggingFace `resolve` URL for a repo file on the default (`main`) revision. A `GET` here
/// 302-redirects to a single-use signed CDN blob URL. [`probe_range`] reads this redirect's own
/// headers *without following it* (they carry the size + sha256); each chunk then re-requests this
/// same stable URL and follows its own fresh redirect (`Range` is preserved across the hop), so no
/// signed CDN URL is ever shared between requests. Public models only — no auth header.
fn hf_resolve_url(repo_id: &str, filename: &str) -> String {
    format!("https://huggingface.co/{repo_id}/resolve/main/{filename}")
}

/// What [`probe_range`] learned: the total size, whether the server honours byte ranges, and the
/// LFS sha256 (normalised, when advertised). The CDN URL is deliberately **not** captured: HF's
/// signed CDN URLs are single-use (consumed by the request that follows the redirect), so each
/// chunk re-requests the stable `resolve` URL and follows its own fresh redirect.
struct RangeInfo {
    total: u64,
    is_partial: bool,
    accept_ranges: bool,
    sha256: Option<String>,
}

/// Probes a HuggingFace `resolve` URL to learn whether the file supports ranged, resumable,
/// parallel downloads, how big it is, and its content sha256. The probe deliberately **does not
/// follow the redirect**: huggingface.co answers the `resolve` request with a `302` whose own
/// headers are the authoritative source — `Accept-Ranges: bytes`, `X-Linked-Size` (the true file
/// size), and `X-Linked-ETag` (the LFS sha256). The CDN response *behind* the redirect is not read,
/// because its `ETag` is the Xet content hash rather than the file sha256 (verifying against it
/// fails every download). A `Range: bytes=0-0` is still sent so a non-LFS file served directly
/// (no redirect) yields a cheap `206`/`200` whose `Content-Range`/`Content-Length` we read the same
/// way. Returns `None` when the probe fails or the size is unknown → single-stream fallback.
async fn probe_range(resolve_url: &str, cfg: &ChunkConfig) -> Option<RangeInfo> {
    let client = reqwest::Client::builder()
        .connect_timeout(cfg.connect_timeout)
        .read_timeout(cfg.read_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let resp = client
        .get(resolve_url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .ok()?;
    let status = resp.status();
    let headers = resp.headers();
    let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let accept_ranges = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("bytes"));
    // The resolve `302` reports the file size in `X-Linked-Size`; a directly-served `206` in
    // `Content-Range: bytes 0-0/N`; a directly-served `200` in `Content-Length`.
    let total = x_linked_size(headers)
        .or_else(|| content_range_total(headers))
        .or_else(|| content_length(headers))?;
    let sha256 = lfs_sha256(headers);
    Some(RangeInfo {
        total,
        is_partial,
        accept_ranges,
        sha256,
    })
}

/// Whether a probe response means we can do a chunked (ranged) download — the pure core of the
/// chunked-vs-single-stream decision, unit-tested without network: a `206 Partial Content` (or
/// an explicit `Accept-Ranges: bytes`) plus a known, non-zero total length.
fn range_plan(is_partial: bool, accept_ranges: bool, total: u64) -> bool {
    (is_partial || accept_ranges) && total > 0
}

fn content_length(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Total size from a `Content-Range: bytes 0-0/12345` header (the `/N` suffix). `*` (unknown) and
/// malformed values yield `None`.
fn content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let v = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let total = v.rsplit('/').next()?.trim();
    (total != "*").then(|| total.parse().ok()).flatten()
}

/// True file size from HuggingFace's `X-Linked-Size` header on the `resolve` redirect — the LFS
/// object length, and the authoritative size for the redirect-less probe (the `302` never carries
/// a body-length header of its own).
fn x_linked_size(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("x-linked-size")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The LFS object sha256 of the file content, which HuggingFace exposes **only** via the
/// `X-Linked-ETag` header on the `resolve` redirect. The bare `ETag` is deliberately *not* a
/// fallback: on a Xet-backed repo the CDN blob's `ETag` is the *Xet content hash* (a different
/// 64-hex digest of the same bytes, surfaced separately as `X-Xet-Hash`), and on classic LFS it is
/// an S3 multipart validator (`md5-partcount`) — neither equals the file sha256, so trusting it
/// would reject every correct download. Returned only when it is a clean 64-hex digest; used for
/// opportunistic integrity verification (a missing/garbled value degrades to the byte-length check).
fn lfs_sha256(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("x-linked-etag")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_sha256)
}

/// Extracts a 64-hex-char sha256 from an `ETag`/`X-Linked-Etag` value, which may be quoted,
/// weak-validator-prefixed (`W/"…"`), or `sha256:`-prefixed. Returns `None` for anything that
/// isn't a clean 64-hex digest — we verify only when we are sure what we have.
fn parse_sha256(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = s.strip_prefix("W/").unwrap_or(s);
    let s = s.trim_matches('"');
    let s = s.strip_prefix("sha256:").unwrap_or(s);
    let s = s.trim_matches('"');
    (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
}

/// Byte length of chunk `index`: `chunk_size`, except the final chunk which holds the remainder.
fn chunk_byte_len(index: usize, chunk_size: u64, total: u64) -> u64 {
    let start = index as u64 * chunk_size;
    (start + chunk_size).min(total) - start
}

/// Downloads `info.effective_url` into `dir/<base>` using `cfg.conns` parallel HTTP `Range`
/// requests writing into one pre-allocated `<base>.part`, resuming any chunks a prior run
/// already completed (tracked in the `<base>.parts` bitmap). Real downloaded bytes accrue into
/// `downloaded` for the progress watchdog — including the resume prefix, added once up front. On
/// success the file is verified, atomically renamed to `<base>`, and the bitmap removed; on any
/// error the `.part` + bitmap are left on disk so the next attempt resumes. The chunk-streaming
/// phase feeds the counter continuously (no `copying` pause); only the no-network finalize phase
/// (fsync + optional hash) sets `copying` so the stall watchdog ignores it.
async fn chunked_download(
    info: &RangeInfo,
    resolve_url: &str,
    dir: &Path,
    base: &str,
    downloaded: &Arc<AtomicU64>,
    copying: &Arc<AtomicBool>,
    cfg: &ChunkConfig,
) -> Result<()> {
    let part = dir.join(format!("{base}.part"));
    let manifest_path = dir.join(format!("{base}.parts"));
    let total = info.total;
    let chunk_size = cfg.chunk_size;
    let chunk_count = total.div_ceil(chunk_size) as usize;

    let (part_file, part_unbacked) = open_preallocated(&part, total).await?;
    let file = Arc::new(part_file);
    let mut manifest =
        Manifest::load_or_init(&manifest_path, total, chunk_size, chunk_count).await?;

    // A `.part` that doesn't already hold `total` real bytes cannot back a `done` mark — there are
    // only zeros there. `open_preallocated` reports this (`part_unbacked`) for both a brand-new
    // (zero-filled) part and a pre-existing one an external truncation shrank, which `set_len`
    // re-grows with zeros (#69). A header-matching manifest that survived over such a part is stale:
    // either a prior run's post-publish cleanup of the bitmap silently failed (all-done), or an
    // interrupted download left a partial bitmap whose multi-GB `.part` a user/cleanup tool later
    // reclaimed (partly-done). Trusting *any* completed bit skips that range, leaving zeros in the
    // assembled file: the length check passes (`set_len` gives exactly `total` bytes) and the sha256
    // check is skipped whenever the CDN advertised no `X-Linked-ETag`. Re-initialise so every chunk
    // refetches. (A legitimate resume is preallocated to `total`, so `part_unbacked` is false.)
    if part_unbacked && manifest.any_complete() {
        manifest = Manifest::reinit(&manifest_path, total, chunk_size, chunk_count).await?;
    }

    // Seed the bar with bytes already on disk (mirrors hf-hub's resume-prefix `update(start)`).
    let already = manifest.completed_bytes(chunk_size, total);
    if already > 0 {
        downloaded.fetch_add(already, Ordering::Relaxed);
    }

    let pending = manifest.pending_indices();
    if !pending.is_empty() {
        let client = Arc::new(
            reqwest::Client::builder()
                .connect_timeout(cfg.connect_timeout)
                .read_timeout(cfg.read_timeout)
                .build()
                .context("build chunked download http client")?,
        );
        let manifest = Arc::new(AsyncMutex::new(manifest));

        // `buffer_unordered` runs up to `conns` chunk futures at once, awaited *inside* this task
        // so the watchdog's `task.abort()` drops every in-flight chunk (no orphaned writers —
        // the gap-#46 fix for this path). `?` on the first error stops scheduling new chunks; the
        // `.part` + bitmap survive for resume.
        let mut stream = stream::iter(pending.into_iter().map(|index| {
            let client = client.clone();
            let file = file.clone();
            let manifest = manifest.clone();
            let downloaded = downloaded.clone();
            async move {
                fetch_chunk(
                    &client,
                    resolve_url,
                    &file,
                    index,
                    chunk_size,
                    total,
                    &downloaded,
                    &manifest,
                )
                .await
            }
        }))
        .buffer_unordered(cfg.conns);
        while let Some(result) = stream.next().await {
            result?;
        }
    }

    verify_and_publish(file, &part, &manifest_path, dir, base, total, info, copying).await
}

/// Opens (creating if absent) the `.part` file for read+write and ensures it is `total` bytes
/// long via `set_len`, so every chunk can `seek_write` at its fixed offset without concurrently
/// extending the file. Returns `(file, unbacked)` where `unbacked` is `true` when the part did
/// **not** already hold *exactly* `total` real bytes — it was just created (length 0), an external
/// truncation left it shorter (so `set_len` re-grows the tail with zeros), or external corruption
/// left it longer (gap #69). In any of those a header-matching `.parts` bitmap cannot be trusted:
/// its done-marks may cover bytes that aren't real, so the caller discards it. A legitimate
/// interrupted download is always preallocated to exactly `total`, so it reports `unbacked = false`
/// and resumes normally. `create_new` makes the create-vs-existed decision atomic (no `exists()`
/// TOCTOU). Off the async runtime — file allocation is blocking I/O.
async fn open_preallocated(part: &Path, total: u64) -> Result<(std::fs::File, bool)> {
    let part = part.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<(std::fs::File, bool)> {
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&part)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&part)
                .with_context(|| format!("open {}", part.display()))?,
            Err(e) => return Err(e).with_context(|| format!("create {}", part.display())),
        };
        // A part can back a `done` mark only if it already holds *exactly* `total` real bytes. Any
        // other on-disk length means it isn't in the state a resume left it: shorter (brand-new at 0,
        // or externally truncated — `set_len` re-grows the tail with zeros) or longer (external
        // corruption/append). A legitimate interrupted download is always preallocated to exactly
        // `total`, so only these anomalies trip `unbacked` — report it so the caller discards a stale
        // bitmap rather than trusting done-marks the bytes can't back (#69).
        let pre_len = file.metadata()?.len();
        let unbacked = pre_len != total;
        if pre_len != total {
            file.set_len(total)
                .with_context(|| format!("preallocate {} to {total} bytes", part.display()))?;
        }
        Ok((file, unbacked))
    })
    .await
    .context("open part-file task failed")?
}

/// Writes the whole buffer at `offset` using positioned writes (no shared seek cursor, so
/// concurrent chunk writers to non-overlapping regions never race). Loops over short writes.
fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    while !buf.is_empty() {
        let n = file.seek_write(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "seek_write wrote 0 bytes",
            ));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Writes the coalesced `buf` to `file` at `offset` off the async runtime (draining `buf` and
/// leaving it empty-but-preallocated for reuse), returning the advanced offset. A no-op when empty.
async fn flush_chunk_writes(
    file: &Arc<std::fs::File>,
    buf: &mut Vec<u8>,
    offset: u64,
    index: usize,
) -> Result<u64> {
    if buf.is_empty() {
        return Ok(offset);
    }
    let bytes = std::mem::replace(buf, Vec::with_capacity(CHUNK_WRITE_BUFFER));
    let count = bytes.len() as u64;
    let file = file.clone();
    tokio::task::spawn_blocking(move || write_all_at(&file, &bytes, offset))
        .await
        .context("chunk write task failed")?
        .with_context(|| format!("write chunk {index} at {offset}"))?;
    Ok(offset + count)
}

/// Downloads chunk `index` (`[index·chunk_size, min(end, total))`) via a single `Range` request
/// and writes it into the shared part file at the chunk's offset. The request targets the stable
/// `resolve_url` and **follows the redirect per request** (`Range` is preserved across redirects),
/// so each chunk consumes its own fresh single-use HF signed CDN URL rather than sharing one. A
/// `206` is required; a transient failure — a `403`/`401`/`429`/`5xx`, or a network-level error
/// (dropped connection / timeout / DNS blip) — is retried with backoff (a fresh redirect each
/// time); a `200` means the server ignored the range and would corrupt the file, so we error
/// rather than write a full body at a chunk offset. Bytes accrue into `downloaded` as they arrive
/// and are coalesced into ~`CHUNK_WRITE_BUFFER` writes; on full receipt the file data is fsync'd
/// **before** the chunk is marked complete — a crash must never mark a chunk whose bytes are still
/// only in the OS cache.
#[allow(clippy::too_many_arguments)]
async fn fetch_chunk(
    client: &reqwest::Client,
    resolve_url: &str,
    file: &Arc<std::fs::File>,
    index: usize,
    chunk_size: u64,
    total: u64,
    downloaded: &Arc<AtomicU64>,
    manifest: &Arc<AsyncMutex<Manifest>>,
) -> Result<()> {
    let start = index as u64 * chunk_size;
    let len = chunk_byte_len(index, chunk_size, total);
    let end = start + len - 1; // inclusive

    let mut attempt: u32 = 0;
    let resp = loop {
        let send_result = client
            .get(resolve_url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
            .send()
            .await;
        match send_result {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::PARTIAL_CONTENT {
                    break resp;
                }
                // A single-use signed CDN URL can race (403/401), or the CDN can throttle
                // (429/5xx). Re-requesting the resolve URL mints a fresh redirect, so retry.
                let transient = status == reqwest::StatusCode::FORBIDDEN
                    || status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status.is_server_error();
                if transient && attempt < CHUNK_RETRY_MAX_ATTEMPTS {
                    attempt += 1;
                    tracing::warn!(
                        chunk = index,
                        attempt,
                        %status,
                        "chunk request failed transiently; re-resolving and retrying"
                    );
                    tokio::time::sleep(CHUNK_RETRY_BACKOFF * attempt).await;
                    continue;
                }
                // Two distinct terminal failures exit here; keep them distinguishable so a log
                // reader isn't sent chasing a Range-support problem that doesn't exist. A non-206
                // success (e.g. `200`) means the server *ignored* the Range header; an exhausted
                // transient (e.g. `403`/`429`) means it kept failing under retry.
                return Err(if transient {
                    anyhow!(
                        "chunk {index} failed after {CHUNK_RETRY_MAX_ATTEMPTS} retries: status {status}"
                    )
                } else {
                    anyhow!(
                        "server ignored Range for chunk {index}: status {status} (expected 206)"
                    )
                });
            }
            // A dropped connection / timeout / DNS blip is exactly the transient the retry loop is
            // for; a bare `?` here would have failed the whole download on the first network hiccup.
            Err(e) => {
                if attempt < CHUNK_RETRY_MAX_ATTEMPTS {
                    attempt += 1;
                    tracing::warn!(
                        chunk = index,
                        attempt,
                        error = %e,
                        "chunk request failed with a network error; re-resolving and retrying"
                    );
                    tokio::time::sleep(CHUNK_RETRY_BACKOFF * attempt).await;
                    continue;
                }
                return Err(e).with_context(|| {
                    format!(
                        "request chunk {index} ({start}-{end}) after {CHUNK_RETRY_MAX_ATTEMPTS} retries"
                    )
                });
            }
        }
    };

    let limit = end + 1; // exclusive upper bound this chunk may write to
    let mut received = start; // absolute position of the next byte to arrive
    let mut written = start; // absolute position of the next byte to write
    let mut buf: Vec<u8> = Vec::with_capacity(CHUNK_WRITE_BUFFER);
    let mut body = resp.bytes_stream();
    while let Some(frame) = body.next().await {
        let frame = frame.with_context(|| format!("read chunk {index} body"))?;
        if frame.is_empty() {
            continue;
        }
        let n = frame.len() as u64;
        if received + n > limit {
            return Err(anyhow!("chunk {index} overran its range"));
        }
        received += n;
        downloaded.fetch_add(n, Ordering::Relaxed);
        buf.extend_from_slice(&frame);
        if buf.len() >= CHUNK_WRITE_BUFFER {
            written = flush_chunk_writes(file, &mut buf, written, index).await?;
        }
    }
    written = flush_chunk_writes(file, &mut buf, written, index).await?;
    if written != limit {
        return Err(anyhow!(
            "chunk {index} short read: got {}, expected {len}",
            written - start
        ));
    }

    // Durability ordering: data first, then the completion mark.
    let f = file.clone();
    tokio::task::spawn_blocking(move || f.sync_data())
        .await
        .context("chunk fsync task failed")?
        .context("fsync chunk data")?;
    manifest.lock().await.mark_complete(index).await
}

/// Verifies the assembled part file and atomically publishes it into the clean layout, then
/// removes the resume bitmap. Integrity is the LFS sha256 when the CDN advertised one
/// (strongest), else the byte length — a missing/garbled hash never fails the download. The
/// finalize phase streams no new bytes, so `copying` pauses the network stall watchdog (a
/// multi-GB fsync + hash on a slow disk can outlast its window).
#[allow(clippy::too_many_arguments)]
async fn verify_and_publish(
    file: Arc<std::fs::File>,
    part: &Path,
    manifest_path: &Path,
    dir: &Path,
    base: &str,
    total: u64,
    info: &RangeInfo,
    copying: &Arc<AtomicBool>,
) -> Result<()> {
    copying.store(true, Ordering::Relaxed);
    let result = verify_and_publish_inner(file, part, manifest_path, dir, base, total, info).await;
    copying.store(false, Ordering::Relaxed);
    result
}

async fn verify_and_publish_inner(
    file: Arc<std::fs::File>,
    part: &Path,
    manifest_path: &Path,
    dir: &Path,
    base: &str,
    total: u64,
    info: &RangeInfo,
) -> Result<()> {
    // Flush everything before the integrity read, then release the handle so the rename can't
    // race an open writer.
    tokio::task::spawn_blocking(move || file.sync_all())
        .await
        .context("final fsync task failed")?
        .context("fsync assembled file")?;

    let part_buf = part.to_path_buf();
    let expected_sha = info.sha256.clone();
    let verify = tokio::task::spawn_blocking(move || -> Result<()> {
        let len = std::fs::metadata(&part_buf)
            .with_context(|| format!("stat {}", part_buf.display()))?
            .len();
        if len != total {
            return Err(anyhow!(
                "assembled {} is {len} bytes, expected {total}",
                part_buf.display()
            ));
        }
        if let Some(expected) = expected_sha {
            let actual =
                sha256_file(&part_buf).with_context(|| format!("hash {}", part_buf.display()))?;
            if actual != expected {
                return Err(anyhow!(
                    "sha256 mismatch for {}: got {actual}, expected {expected}",
                    part_buf.display()
                ));
            }
        }
        Ok(())
    })
    .await
    .context("verify task failed")?;
    if let Err(e) = verify {
        // A corrupt assembly (or a wrong advertised hash) won't fix itself on resume — every
        // chunk is already marked done — so discard the partial to force a clean re-download.
        let _ = tokio::fs::remove_file(part).await;
        let _ = tokio::fs::remove_file(manifest_path).await;
        return Err(e);
    }

    let dest = dir.join(base);
    tokio::fs::rename(part, &dest)
        .await
        .with_context(|| format!("publish {}", dest.display()))?;
    let _ = tokio::fs::remove_file(manifest_path).await;
    Ok(())
}

/// Streams a file through sha256 (constant memory — the model is multi-GB).
fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// Per-chunk completion bitmap persisted next to the `.part` so a download resumes across process
/// restarts. On-disk layout: a header line `SSV2CPARTS v1 {total} {chunk_size} {chunk_count}\n`
/// then one byte per chunk (`b'1'` = complete). Marking a chunk writes its single byte and fsyncs
/// — atomic per chunk, no full rewrite, no torn-write window. A header that doesn't match the
/// current download means the partial belongs to a different upstream file → start clean.
struct Manifest {
    path: PathBuf,
    chunk_count: usize,
    done: Vec<bool>,
    header_len: u64,
}

impl Manifest {
    fn header_for(total: u64, chunk_size: u64, chunk_count: usize) -> String {
        format!("SSV2CPARTS v1 {total} {chunk_size} {chunk_count}\n")
    }

    async fn load_or_init(
        path: &Path,
        total: u64,
        chunk_size: u64,
        chunk_count: usize,
    ) -> Result<Manifest> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            Manifest::load_or_init_sync(&path, total, chunk_size, chunk_count)
        })
        .await
        .context("load parts manifest task failed")?
    }

    fn load_or_init_sync(
        path: &Path,
        total: u64,
        chunk_size: u64,
        chunk_count: usize,
    ) -> Result<Manifest> {
        let header = Manifest::header_for(total, chunk_size, chunk_count);
        match std::fs::read(path) {
            Ok(bytes) => {
                // Only trust a bitmap whose header matches this exact download; otherwise it is a
                // stale partial for a different file and is re-initialised below.
                if bytes.starts_with(header.as_bytes()) && bytes.len() == header.len() + chunk_count
                {
                    let done = bytes[header.len()..].iter().map(|b| *b == b'1').collect();
                    return Ok(Manifest {
                        path: path.to_path_buf(),
                        chunk_count,
                        done,
                        header_len: header.len() as u64,
                    });
                }
            }
            // No manifest yet → start a fresh download.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // A manifest that *exists* but won't read (a Windows sharing violation from AV / the
            // search indexer, a transient permissions hiccup) must not be treated as absent:
            // re-initialising would truncate a valid bitmap and silently discard real download
            // progress. Surface it so the job-queue retries the whole job instead.
            Err(e) => {
                return Err(e).with_context(|| format!("read parts manifest {}", path.display()));
            }
        }
        Manifest::init_sync(path, total, chunk_size, chunk_count)
    }

    /// Forcibly writes a fresh all-zero bitmap, discarding any existing one. Used for the first
    /// download and to drop a stale all-done manifest left over a brand-new (zero-filled) `.part`.
    fn init_sync(path: &Path, total: u64, chunk_size: u64, chunk_count: usize) -> Result<Manifest> {
        let header = Manifest::header_for(total, chunk_size, chunk_count);
        let mut init = Vec::with_capacity(header.len() + chunk_count);
        init.extend_from_slice(header.as_bytes());
        init.resize(header.len() + chunk_count, b'0');
        write_file_durable(path, &init)
            .with_context(|| format!("init parts manifest {}", path.display()))?;
        Ok(Manifest {
            path: path.to_path_buf(),
            chunk_count,
            done: vec![false; chunk_count],
            header_len: header.len() as u64,
        })
    }

    /// Off-runtime [`Manifest::init_sync`] — re-initialises the bitmap to all-pending.
    async fn reinit(
        path: &Path,
        total: u64,
        chunk_size: u64,
        chunk_count: usize,
    ) -> Result<Manifest> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            Manifest::init_sync(&path, total, chunk_size, chunk_count)
        })
        .await
        .context("reinit parts manifest task failed")?
    }

    fn pending_indices(&self) -> Vec<usize> {
        (0..self.chunk_count).filter(|i| !self.done[*i]).collect()
    }

    /// True when the bitmap marks at least one chunk complete. Over a brand-new (zero-filled)
    /// `.part`, any such mark is necessarily stale — there are no real bytes on disk to back it.
    fn any_complete(&self) -> bool {
        self.done.contains(&true)
    }

    fn completed_bytes(&self, chunk_size: u64, total: u64) -> u64 {
        (0..self.chunk_count)
            .filter(|i| self.done[*i])
            .map(|i| chunk_byte_len(i, chunk_size, total))
            .sum()
    }

    async fn mark_complete(&mut self, index: usize) -> Result<()> {
        if self.done[index] {
            return Ok(());
        }
        let path = self.path.clone();
        let offset = self.header_len + index as u64;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .with_context(|| format!("open parts manifest {}", path.display()))?;
            write_all_at(&f, b"1", offset)?;
            f.sync_data().context("fsync parts manifest")?;
            Ok(())
        })
        .await
        .context("mark-complete task failed")??;
        self.done[index] = true;
        Ok(())
    }
}

/// Writes `bytes` to `path` (truncating) and fsyncs — used to initialise the parts bitmap.
fn write_file_durable(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    f.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    f.sync_data()
        .with_context(|| format!("fsync {}", path.display()))?;
    Ok(())
}

/// Diagnostic entry point for `examples/repro_8b.rs` (not used by the app): download a single repo
/// file into `cache_dir`, retrying with the same lock-contention backoff [`fetch_one`]'s
/// single-stream fallback uses, so the concurrent-instance race can be exercised without driving the
/// whole app. Returns the cached blob path (no clean-layout placement). Mirrors the production
/// backoff but omits the post-backoff cache re-check, which needs the clean-layout context
/// `fetch_one` owns.
#[doc(hidden)]
pub async fn download_file_with_lock_retry_for_diagnostics(
    cache_dir: &Path,
    repo_id: &str,
    filename: &str,
) -> Result<PathBuf> {
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(cache_dir.to_path_buf())
        .build()
        .context("build hf-hub api")?;
    let repo_api = api.model(repo_id.to_string());
    let downloaded = Arc::new(AtomicU64::new(0));
    let mut attempt: u32 = 0;
    loop {
        let progress = ByteCounter {
            downloaded: downloaded.clone(),
        };
        match repo_api.download_with_progress(filename, progress).await {
            Ok(path) => return Ok(path),
            Err(e)
                if matches!(e, ApiError::LockAcquisition(_))
                    && attempt < LOCK_RETRY_MAX_ATTEMPTS =>
            {
                attempt += 1;
                let backoff = (LOCK_RETRY_BACKOFF * attempt).min(LOCK_RETRY_BACKOFF_CAP);
                tracing::warn!(
                    filename,
                    attempt,
                    backoff_secs = backoff.as_secs(),
                    "model file lock held by another download; backing off and retrying"
                );
                tokio::time::sleep(backoff).await;
            }
            Err(e) => return Err(e).with_context(|| format!("download {filename}")),
        }
    }
}

/// Number of `poll`-interval watchdog ticks with no new bytes that constitutes a stall
/// (`timeout / poll`, never zero).
fn stall_limit(timeout: Duration, poll: Duration) -> u32 {
    let poll_ms = poll.as_millis().max(1);
    (timeout.as_millis() / poll_ms).max(1) as u32
}

/// Advances the stall counter for one watchdog tick: resets to 0 (and raises the high-water
/// mark) when bytes grew, otherwise increments. Returns the new `(last_seen, stale)` pair.
fn stall_step(downloaded: u64, last_seen: u64, stale: u32) -> (u64, u32) {
    if downloaded > last_seen {
        (downloaded, 0)
    } else {
        (last_seen, stale.saturating_add(1))
    }
}

/// Best-effort total download size for a repo's selected files, via the HF tree API. Used
/// only to render a percentage — `None` (size unknown) just falls back to a byte counter.
async fn total_download_bytes(repo_id: &str, needs_mmproj: bool) -> Option<u64> {
    #[derive(Deserialize)]
    struct TreeEntry {
        path: String,
        #[serde(default)]
        size: u64,
    }
    let url = format!("https://huggingface.co/api/models/{repo_id}/tree/main");
    let client = reqwest::Client::builder()
        .timeout(HTTP_API_TIMEOUT)
        .build()
        .ok()?;
    let resp = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let entries: Vec<TreeEntry> = resp.json().await.ok()?;
    let names: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let (gguf, mmproj) = pick_gguf_files(&names, needs_mmproj);
    let size_of = |name: &Option<String>| -> u64 {
        name.as_ref()
            .and_then(|n| entries.iter().find(|e| &e.path == n))
            .map(|e| e.size)
            .unwrap_or(0)
    };
    let total = size_of(&gguf) + if needs_mmproj { size_of(&mmproj) } else { 0 };
    (total > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    static ENV_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    #[tokio::test]
    async fn rate_limited_release_lookup_uses_direct_fallback() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases"))
            .and(wiremock::matchers::header("user-agent", USER_AGENT))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-limit", "60")
                    .insert_header("x-ratelimit-remaining", "0")
                    .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (url, name) =
            resolve_binary_url_with_fallback(&format!("{}/releases", server.uri())).await;
        assert_eq!(url, FALLBACK_VULKAN_ASSET.0);
        assert_eq!(name, FALLBACK_VULKAN_ASSET.1);
        assert_eq!(pick_vulkan_asset(&[name]), Some(0));
    }

    #[tokio::test]
    async fn release_lookup_keeps_newest_matching_asset_when_api_works() {
        let server = MockServer::start().await;
        let url = "https://github.com/ggml-org/llama.cpp/releases/download/b2/llama-b2-bin-win-vulkan-x64.zip";
        Mock::given(method("GET"))
            .and(path("/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"assets": [{"name": "llama-b3-xcframework.zip", "browser_download_url": "https://example.invalid/mac.zip"}]},
                {"assets": [{"name": "llama-b2-bin-win-vulkan-x64.zip", "browser_download_url": url}]}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let (found, name) =
            resolve_binary_url_with_fallback(&format!("{}/releases", server.uri())).await;
        assert_eq!(
            found, url,
            "a working API must not be overridden by the pin"
        );
        assert_eq!(name, "llama-b2-bin-win-vulkan-x64.zip");
    }

    #[tokio::test]
    async fn binary_download_retries_a_transient_status_then_succeeds() {
        let server = MockServer::start().await;
        // A single-use signed-CDN style 403, then the real body.
        Mock::given(method("GET"))
            .and(path("/llama.zip"))
            .respond_with(ResponseTemplate::new(403))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/llama.zip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(zip_with_llama_server(b"retried"), "application/zip"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let root = unique_temp_dir("binary-retry");
        let install = root.join("llama");
        let found = ensure_binary_from_url_verified(
            &install,
            &format!("{}/llama.zip", server.uri()),
            "x.zip",
            None,
        )
        .await
        .expect("a transient 403 must be retried, not fatal");
        assert_eq!(std::fs::read(&found).unwrap(), b"retried");
        assert!(!partial_install_dir(&install).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn binary_download_reports_persistent_failure_after_bounded_attempts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/llama.zip"))
            .respond_with(ResponseTemplate::new(403))
            .expect(u64::from(BINARY_RETRY_MAX_ATTEMPTS))
            .mount(&server)
            .await;

        let root = unique_temp_dir("binary-fail");
        let install = root.join("llama");
        let err = ensure_binary_from_url_verified(
            &install,
            &format!("{}/llama.zip", server.uri()),
            "x.zip",
            None,
        )
        .await
        .expect_err("a persistent 403 must fail truthfully");
        let message = format!("{err:#}");
        assert!(
            message.contains("download") && message.contains("403"),
            "error must name the failed download and status, got: {message}"
        );
        assert!(
            !install.exists(),
            "a failed download must not publish an install dir"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn pinned_asset_digest_is_verified_before_installing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pinned.zip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(zip_with_llama_server(b"tampered"), "application/zip"),
            )
            .mount(&server)
            .await;

        let root = unique_temp_dir("pinned-digest");
        let install = root.join("llama");
        let err = ensure_binary_from_url_verified(
            &install,
            &format!("{}/pinned.zip", server.uri()),
            "pinned.zip",
            Some(FALLBACK_VULKAN_SHA256),
        )
        .await
        .expect_err("a digest mismatch must refuse the download");
        assert!(
            format!("{err:#}").contains("sha256 mismatch"),
            "unexpected error: {err:#}"
        );
        assert!(
            !install.exists(),
            "an unverified asset must never be installed"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Real first-run acquisition against GitHub (~32 MB). Gated manual check, not part of
    /// the always-green suite: `SSV2C_BIN_TEST_DIR` keeps the install for inspection.
    ///
    /// ```text
    /// cargo test -p inference --lib downloads_the_real_sidecar_binary -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "downloads the real llama.cpp Vulkan release from GitHub (~32 MB)"]
    async fn downloads_the_real_sidecar_binary() {
        let dir = real_binary_test_dir("api-path");
        let binary = ensure_binary(&dir)
            .await
            .expect("ensure the real llama-server binary");
        assert_sidecar_starts(&binary);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The field failure end-to-end: the releases API errors (a nonexistent repo stands in
    /// for the exhausted per-IP quota), the pinned fallback is used, its published digest is
    /// verified, and the extracted `llama-server.exe` actually runs.
    #[tokio::test]
    #[ignore = "downloads the real pinned llama.cpp release from GitHub (~32 MB)"]
    async fn real_api_failure_bootstraps_from_the_pinned_asset() {
        let (url, name) = resolve_binary_url_with_fallback(
            "https://api.github.com/repos/ggml-org/definitely-not-a-repo-xyz/releases?per_page=10",
        )
        .await;
        assert_eq!(url, FALLBACK_VULKAN_ASSET.0, "must fall back to the pin");
        assert_eq!(name, FALLBACK_VULKAN_ASSET.1);

        let dir = real_binary_test_dir("pinned-fallback");
        let install = dir.join("llama");
        let binary = ensure_binary_from_url_verified(&install, &url, &name, fallback_sha256(&url))
            .await
            .expect("real pinned asset must download and verify");
        assert_sidecar_starts(&binary);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Each ignored real-network test gets its own subdirectory: the suite runs test
    /// functions in parallel, and two installs sharing one directory collide on the final
    /// `llama.partial -> llama` rename (observed as `Access is denied`).
    fn real_binary_test_dir(tag: &str) -> PathBuf {
        let dir = std::env::var("SSV2C_BIN_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("ssv2c-binary-smoke"))
            .join(tag);
        std::fs::create_dir_all(&dir).expect("create binary test dir");
        dir
    }

    /// A real binary is only useful if the Windows loader can start it and its DLL set is
    /// complete. Since the b10xxx builds ship `llama-server.exe` as a ~9 KB launcher stub
    /// with the code in `llama-server-impl.dll`, the install directory — not the .exe — is
    /// what must be substantial, and `--help` proves both the loader and the DLL set.
    fn assert_sidecar_starts(binary: &Path) {
        let install = binary.parent().expect("binary parent dir");
        let total: u64 = std::fs::read_dir(install)
            .expect("read install dir")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            install.join("llama-server-impl.dll").exists() || total > 20 * 1024 * 1024,
            "{} looks like a partial extraction ({total} bytes total)",
            install.display()
        );
        let output = std::process::Command::new(binary)
            .arg("--help")
            .output()
            .expect("run llama-server --help");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "llama-server --help failed: status={:?} stderr={stderr}",
            output.status
        );
        assert!(
            stdout.contains("--model") || stderr.contains("--model"),
            "help output did not look like llama-server: {stdout}{stderr}"
        );
        println!(
            "BINARY OK: {} ({} bytes total install)",
            binary.display(),
            total
        );
    }

    #[test]
    fn picks_win_vulkan_x64_asset() {
        let names = vec![
            "llama-b6500-bin-win-cpu-x64.zip".to_string(),
            "llama-b6500-bin-win-vulkan-x64.zip".to_string(),
            "llama-b6500-bin-ubuntu-vulkan-x64.zip".to_string(),
            "cudart-llama-bin-win-cuda-12.4-x64.zip".to_string(),
        ];
        let idx = pick_vulkan_asset(&names).expect("vulkan asset");
        assert_eq!(names[idx], "llama-b6500-bin-win-vulkan-x64.zip");
    }

    #[test]
    fn no_vulkan_asset_returns_none() {
        let names = vec!["llama-b6500-bin-win-cpu-x64.zip".to_string()];
        assert!(pick_vulkan_asset(&names).is_none());
    }

    fn asset(name: &str, url: &str) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            browser_download_url: url.to_string(),
        }
    }

    #[test]
    fn skips_release_with_incomplete_assets() {
        // Mirrors observed llama.cpp behaviour: a freshly-published "latest" release
        // (e.g. b9753) can carry an incomplete asset set with no win-vulkan-x64 zip,
        // while the previous release (b9752) is complete. We must fall back to it.
        let releases = vec![
            GithubRelease {
                assets: vec![asset("llama-b9753-xcframework.zip", "https://x/9753-xcf")],
            },
            GithubRelease {
                assets: vec![
                    asset("llama-b9752-bin-win-cpu-x64.zip", "https://x/9752-cpu"),
                    asset(
                        "llama-b9752-bin-win-vulkan-x64.zip",
                        "https://x/9752-vulkan",
                    ),
                ],
            },
        ];
        let (url, name) =
            pick_vulkan_from_releases(&releases).expect("vulkan from recent releases");
        assert_eq!(name, "llama-b9752-bin-win-vulkan-x64.zip");
        assert_eq!(url, "https://x/9752-vulkan");
    }

    #[test]
    fn prefers_the_newest_release_that_has_vulkan() {
        let releases = vec![
            GithubRelease {
                assets: vec![asset("llama-b2-bin-win-vulkan-x64.zip", "https://x/b2")],
            },
            GithubRelease {
                assets: vec![asset("llama-b1-bin-win-vulkan-x64.zip", "https://x/b1")],
            },
        ];
        let (url, _name) = pick_vulkan_from_releases(&releases).expect("newest vulkan");
        assert_eq!(url, "https://x/b2");
    }

    #[test]
    fn no_vulkan_in_any_release_returns_none() {
        let releases = vec![GithubRelease {
            assets: vec![asset("llama-b1-bin-win-cpu-x64.zip", "https://x/cpu")],
        }];
        assert!(pick_vulkan_from_releases(&releases).is_none());
    }

    #[tokio::test]
    async fn env_override_wins_over_existing_install() {
        let _guard = ENV_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/llama.zip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(zip_with_llama_server(b"override"), "application/zip"),
            )
            .mount(&server)
            .await;

        let sidecar_dir = unique_temp_dir("override");
        let normal_binary = sidecar_dir
            .join("llama")
            .join("existing")
            .join("llama-server.exe");
        std::fs::create_dir_all(normal_binary.parent().unwrap()).unwrap();
        std::fs::write(&normal_binary, b"normal").unwrap();

        let override_url = format!("{}/llama.zip", server.uri());
        std::env::set_var("SSV2C_LLAMA_RELEASE_URL", &override_url);
        let found = ensure_binary(&sidecar_dir)
            .await
            .expect("override binary should resolve");
        std::env::remove_var("SSV2C_LLAMA_RELEASE_URL");

        let override_install = sidecar_dir
            .join("llama-override")
            .join(url_fingerprint(&override_url));
        assert_ne!(
            found, normal_binary,
            "env override must not reuse a normal existing install"
        );
        assert!(
            found.starts_with(&override_install),
            "override binary should live under {} but got {}",
            override_install.display(),
            found.display()
        );
        assert!(
            !partial_install_dir(&override_install).exists(),
            "successful install should not leave a partial directory"
        );
        assert_eq!(std::fs::read(&found).unwrap(), b"override");
        assert_eq!(
            std::fs::read(&normal_binary).unwrap(),
            b"normal",
            "normal install must be preserved"
        );
        let _ = std::fs::remove_dir_all(sidecar_dir);
    }

    #[tokio::test]
    async fn failed_binary_extraction_cleans_partial_install() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/bad.zip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(b"this is not a zip".to_vec(), "application/zip"),
            )
            .mount(&server)
            .await;

        let root = unique_temp_dir("bad-zip");
        let install = root.join("llama");
        let err = ensure_binary_from_url_verified(
            &install,
            &format!("{}/bad.zip", server.uri()),
            "bad.zip",
            None,
        )
        .await
        .expect_err("invalid zip should fail");

        assert!(
            err.to_string().contains("extract llama release zip"),
            "unexpected error: {err:?}"
        );
        assert!(
            !partial_install_dir(&install).exists(),
            "failed extraction must clean up the partial install dir"
        );
        assert!(
            !install.exists(),
            "failed extraction must not publish an install dir"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn installed_binary_candidates_include_normal_and_overrides() {
        let sidecar_dir = unique_temp_dir("candidates");
        let normal = sidecar_dir
            .join("llama")
            .join("release")
            .join("llama-server.exe");
        let override_a = sidecar_dir
            .join("llama-override")
            .join("aaaaaaaaaaaaaaaa")
            .join("bin")
            .join("llama-server.exe");
        let unrelated = sidecar_dir.join("other").join("llama-server.exe");
        for path in [&normal, &override_a, &unrelated] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"exe").unwrap();
        }

        let found = installed_binary_candidates(&sidecar_dir);

        assert!(found.contains(&normal), "missing normal install candidate");
        assert!(
            found.contains(&override_a),
            "missing override install candidate"
        );
        assert!(
            !found.contains(&unrelated),
            "candidate scan must stay inside owned install roots"
        );
        let _ = std::fs::remove_dir_all(sidecar_dir);
    }

    #[tokio::test]
    async fn byte_counter_accumulates_streamed_bytes() {
        // The Progress sink sums every `update` (incl. the resume-prefix `update(start)`).
        let downloaded = Arc::new(AtomicU64::new(0));
        let mut p = ByteCounter {
            downloaded: downloaded.clone(),
        };
        p.init(100, "weights.gguf").await;
        p.update(40).await; // e.g. resumed prefix
        p.update(60).await; // streamed tail
        p.finish().await;
        assert_eq!(downloaded.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn stall_limit_is_timeout_over_poll_and_never_zero() {
        assert_eq!(
            stall_limit(Duration::from_secs(180), Duration::from_millis(750)),
            240
        );
        // Degenerate inputs still yield a usable (>=1) limit rather than an instant abort.
        assert_eq!(
            stall_limit(Duration::from_millis(0), Duration::from_millis(750)),
            1
        );
        assert_eq!(
            stall_limit(Duration::from_secs(5), Duration::from_millis(0)),
            5000
        );
    }

    #[test]
    fn stall_step_resets_on_progress_and_counts_otherwise() {
        // Progress: reset the stale counter, raise the high-water mark.
        assert_eq!(stall_step(50, 10, 3), (50, 0));
        // No progress at the mark: increment, mark unchanged.
        assert_eq!(stall_step(50, 50, 3), (50, 4));
        // A stale read below the mark (shouldn't happen, but must not regress the mark).
        assert_eq!(stall_step(40, 50, 0), (50, 1));
    }

    #[test]
    fn picks_q4_k_m_weights_and_mmproj_for_vision() {
        let files = vec![
            "README.md".to_string(),
            "Qwen3-VL-4B-Instruct-Q8_0.gguf".to_string(),
            "Qwen3-VL-4B-Instruct-Q4_K_M.gguf".to_string(),
            "mmproj-Qwen3-VL-4B-Instruct-f16.gguf".to_string(),
        ];
        let (gguf, mmproj) = pick_gguf_files(&files, true);
        assert_eq!(gguf.as_deref(), Some("Qwen3-VL-4B-Instruct-Q4_K_M.gguf"));
        assert_eq!(
            mmproj.as_deref(),
            Some("mmproj-Qwen3-VL-4B-Instruct-f16.gguf")
        );
    }

    #[test]
    fn answer_repo_needs_no_mmproj() {
        let files = vec!["Ministral-3-3B-Reasoning-Q4_K_M.gguf".to_string()];
        let (gguf, mmproj) = pick_gguf_files(&files, false);
        assert_eq!(
            gguf.as_deref(),
            Some("Ministral-3-3B-Reasoning-Q4_K_M.gguf")
        );
        assert!(mmproj.is_none());
    }

    #[test]
    fn falls_back_to_first_gguf_when_no_q4_k_m() {
        let files = vec![
            "model-Q5_K_M.gguf".to_string(),
            "model-Q8_0.gguf".to_string(),
        ];
        let (gguf, _) = pick_gguf_files(&files, false);
        // Sorted, first is the Q5 — a deterministic fallback.
        assert_eq!(gguf.as_deref(), Some("model-Q5_K_M.gguf"));
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ssv2c-download-{tag}-{}-{suffix}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_with_llama_server(contents: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(
                "llama-b-test/bin/llama-server.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap().into_inner()
    }

    // ── Parallel chunked downloader (PR8) ───────────────────────────────────────────────────

    /// Deterministic pseudo-random bytes (an LCG — `Math.random` is unavailable and we need a
    /// reproducible fixture). Compressible-resistant enough that a misplaced chunk shows up as a
    /// byte-identity failure.
    fn synthetic_blob(len: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(len);
        let mut state: u32 = 0x1234_5678;
        for _ in 0..len {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            v.push((state >> 24) as u8);
        }
        v
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        let digest = h.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for b in digest {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }

    fn test_cfg(conns: usize, chunk_size: u64) -> ChunkConfig {
        ChunkConfig {
            conns,
            chunk_size,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
        }
    }

    fn test_info(total: u64, sha256: Option<String>) -> RangeInfo {
        RangeInfo {
            total,
            is_partial: true,
            accept_ranges: true,
            sha256,
        }
    }

    /// Parses a `bytes=START-END` request range (both bounds always present in our requests).
    fn parse_byte_range(raw: &str) -> Option<(u64, u64)> {
        let spec = raw.trim().strip_prefix("bytes=")?;
        let (s, e) = spec.split_once('-')?;
        Some((s.trim().parse().ok()?, e.trim().parse().ok()?))
    }

    /// A wiremock responder that honours `Range`: a ranged request gets `206` + the slice +
    /// `Content-Range`/`Accept-Ranges`; an un-ranged request gets the full `200` body. It logs
    /// every served range so a test can assert which chunks were (not) fetched.
    struct RangeResponder {
        body: Vec<u8>,
        log: Arc<std::sync::Mutex<Vec<(u64, u64)>>>,
    }

    impl wiremock::Respond for RangeResponder {
        fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
            let range = req
                .headers
                .get("range")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_byte_range);
            match range {
                Some((start, end)) => {
                    let end = end.min(self.body.len() as u64 - 1);
                    self.log.lock().unwrap().push((start, end));
                    let slice = self.body[start as usize..=end as usize].to_vec();
                    let content_range = format!("bytes {start}-{end}/{}", self.body.len());
                    ResponseTemplate::new(206)
                        .insert_header("Accept-Ranges", "bytes")
                        .insert_header("Content-Range", content_range.as_str())
                        .set_body_raw(slice, "application/octet-stream")
                }
                None => ResponseTemplate::new(200)
                    .insert_header("Accept-Ranges", "bytes")
                    .set_body_raw(self.body.clone(), "application/octet-stream"),
            }
        }
    }

    /// A responder that returns `403` for its first `fail_first` requests (the single-use signed-URL
    /// race we hit in production) and serves the range thereafter — to exercise the bounded retry.
    struct FlakyResponder {
        body: Vec<u8>,
        fail_first: u32,
        seen: Arc<std::sync::atomic::AtomicU32>,
    }

    impl wiremock::Respond for FlakyResponder {
        fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
            if self.seen.fetch_add(1, Ordering::Relaxed) < self.fail_first {
                return ResponseTemplate::new(403).set_body_string("denied");
            }
            match req
                .headers
                .get("range")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_byte_range)
            {
                Some((start, end)) => {
                    let end = end.min(self.body.len() as u64 - 1);
                    let slice = self.body[start as usize..=end as usize].to_vec();
                    let content_range = format!("bytes {start}-{end}/{}", self.body.len());
                    ResponseTemplate::new(206)
                        .insert_header("Accept-Ranges", "bytes")
                        .insert_header("Content-Range", content_range.as_str())
                        .set_body_raw(slice, "application/octet-stream")
                }
                None => ResponseTemplate::new(200)
                    .set_body_raw(self.body.clone(), "application/octet-stream"),
            }
        }
    }

    async fn mount_range_server(
        body: Vec<u8>,
    ) -> (MockServer, Arc<std::sync::Mutex<Vec<(u64, u64)>>>) {
        let server = MockServer::start().await;
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        Mock::given(method("GET"))
            .and(path("/blob"))
            .respond_with(RangeResponder {
                body,
                log: log.clone(),
            })
            .mount(&server)
            .await;
        (server, log)
    }

    #[test]
    fn range_plan_requires_ranges_and_known_size() {
        assert!(range_plan(true, false, 1000), "206 alone is enough");
        assert!(
            range_plan(false, true, 1000),
            "Accept-Ranges alone is enough"
        );
        assert!(
            !range_plan(false, false, 1000),
            "no range support → single-stream"
        );
        assert!(!range_plan(true, true, 0), "unknown size → single-stream");
    }

    #[test]
    fn parse_sha256_normalizes_etag_forms() {
        let hex = "a".repeat(64);
        assert_eq!(parse_sha256(&format!("\"{hex}\"")), Some(hex.clone()));
        assert_eq!(
            parse_sha256(&format!("W/\"sha256:{}\"", hex.to_uppercase())),
            Some(hex.clone()),
            "weak validator + sha256: prefix + uppercase normalizes"
        );
        assert_eq!(parse_sha256("\"deadbeef\""), None, "too short");
        assert_eq!(parse_sha256("not-a-hash-value-xyz"), None);
    }

    #[test]
    fn lfs_sha256_trusts_only_x_linked_etag_not_cdn_etag() {
        use reqwest::header::{HeaderMap, HeaderValue};
        // Real-world Xet-backed file: the resolve `302` carries the true file sha256 in
        // `X-Linked-ETag`, while the CDN blob's bare `ETag` is the *Xet content hash* — a different
        // 64-hex digest of the same bytes. Trusting the bare `ETag` made every verification fail.
        let real = "66358cb18bb6b3b1b6675aa412c7a88ef01d228f481184d13668e5201c730a0a";
        let xet = "d4ccbe2aafe6a38e45695e8414f071078f894e3aa59426454d3242e5944397c5";

        // The CDN response behind the redirect carries only a bare `ETag` (the Xet hash). It must
        // NOT be treated as the file sha256 — better to skip integrity than to verify against the
        // wrong digest and reject every (correct) download.
        let mut cdn = HeaderMap::new();
        cdn.insert(
            "etag",
            HeaderValue::from_str(&format!("\"{xet}\"")).unwrap(),
        );
        assert_eq!(
            lfs_sha256(&cdn),
            None,
            "a bare CDN ETag is not the file sha256 and must be ignored"
        );

        // The resolve `302` carries `X-Linked-ETag` — the real sha256 — even alongside a (different)
        // bare `ETag`. That is the value we verify against.
        let mut redirect = HeaderMap::new();
        redirect.insert(
            "x-linked-etag",
            HeaderValue::from_str(&format!("\"{real}\"")).unwrap(),
        );
        redirect.insert(
            "etag",
            HeaderValue::from_str(&format!("\"{xet}\"")).unwrap(),
        );
        assert_eq!(
            lfs_sha256(&redirect),
            Some(real.to_string()),
            "X-Linked-ETag is the file sha256"
        );
    }

    #[test]
    fn content_range_total_parses_suffix() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::CONTENT_RANGE,
            "bytes 0-0/123456".parse().unwrap(),
        );
        assert_eq!(content_range_total(&h), Some(123456));
        h.insert(
            reqwest::header::CONTENT_RANGE,
            "bytes 0-0/*".parse().unwrap(),
        );
        assert_eq!(content_range_total(&h), None, "unknown total → None");
    }

    #[tokio::test]
    async fn chunked_download_assembles_byte_identical_file() {
        let blob = synthetic_blob(700 * 1024);
        let (server, _log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-integrity");
        let url = format!("{}/blob", server.uri());
        let info = test_info(blob.len() as u64, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, 64 * 1024); // ~11 chunks across 4 connections

        chunked_download(&info, &url, &dir, "blob.gguf", &downloaded, &copying, &cfg)
            .await
            .expect("chunked download should succeed");

        let out = std::fs::read(dir.join("blob.gguf")).unwrap();
        assert_eq!(
            out, blob,
            "assembled file must be byte-identical to the source"
        );
        assert_eq!(
            downloaded.load(Ordering::Relaxed),
            blob.len() as u64,
            "progress counter must equal the total"
        );
        assert!(
            !dir.join("blob.gguf.part").exists(),
            "the .part must be renamed away on success"
        );
        assert!(
            !dir.join("blob.gguf.parts").exists(),
            "the resume bitmap must be removed on success"
        );
        assert!(
            !copying.load(Ordering::Relaxed),
            "copying flag must be cleared after finalize"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn chunk_requests_follow_redirect_and_preserve_range() {
        // The real bug: HF's `resolve` URL 302-redirects to a *single-use* signed CDN URL, so each
        // chunk must re-request the resolve URL and follow its own redirect — with `Range` surviving
        // the (cross-origin) hop. Two servers stand in for huggingface.co → CDN. If `Range` were
        // dropped on the redirect, the CDN would return a 200 full body and `fetch_chunk` would error.
        let blob = synthetic_blob(300 * 1024);
        let cdn = MockServer::start().await;
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        Mock::given(method("GET"))
            .and(path("/cdn"))
            .respond_with(RangeResponder {
                body: blob.clone(),
                log: log.clone(),
            })
            .mount(&cdn)
            .await;
        let resolve = MockServer::start().await;
        let cdn_url = format!("{}/cdn", cdn.uri());
        Mock::given(method("GET"))
            .and(path("/resolve"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", cdn_url.as_str()))
            .mount(&resolve)
            .await;

        let dir = unique_temp_dir("chunk-redirect");
        let url = format!("{}/resolve", resolve.uri());
        let info = test_info(blob.len() as u64, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, 48 * 1024);

        chunked_download(&info, &url, &dir, "r.gguf", &downloaded, &copying, &cfg)
            .await
            .expect("a chunked download via a redirecting resolve URL should succeed");

        let out = std::fs::read(dir.join("r.gguf")).unwrap();
        assert_eq!(
            out, blob,
            "file assembled through the redirect must be byte-identical"
        );
        assert!(
            !log.lock().unwrap().is_empty(),
            "the CDN must have served ranged requests (Range survived the redirect → 206)"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn chunk_retries_transient_403_then_succeeds() {
        // The production failure mode (single-use signed-URL race) but recoverable: the first
        // request 403s, the retry succeeds. The download must not treat the 403 as fatal.
        let blob = synthetic_blob(40 * 1024);
        let server = MockServer::start().await;
        let seen = Arc::new(std::sync::atomic::AtomicU32::new(0));
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(FlakyResponder {
                body: blob.clone(),
                fail_first: 1,
                seen: seen.clone(),
            })
            .mount(&server)
            .await;
        let dir = unique_temp_dir("chunk-retry");
        let url = format!("{}/flaky", server.uri());
        let info = test_info(blob.len() as u64, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(1, blob.len() as u64); // a single chunk

        chunked_download(&info, &url, &dir, "flaky.gguf", &downloaded, &copying, &cfg)
            .await
            .expect("a transient 403 must be retried, not fatal");

        assert_eq!(std::fs::read(dir.join("flaky.gguf")).unwrap(), blob);
        assert!(
            seen.load(Ordering::Relaxed) >= 2,
            "the chunk should have been retried after the 403"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn resume_skips_already_completed_chunks() {
        let blob = synthetic_blob(500 * 1024);
        let chunk_size = 64 * 1024u64;
        let total = blob.len() as u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;
        let (server, log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-resume");
        let base = "model.gguf";

        // Pre-seed: pre-allocate the part, write chunk 0's correct bytes, mark chunk 0 done.
        let part = dir.join(format!("{base}.part"));
        {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&part)
                .unwrap();
            f.set_len(total).unwrap();
            let c0 = chunk_byte_len(0, chunk_size, total) as usize;
            f.seek_write(&blob[..c0], 0).unwrap();
        }
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            let mut marks = vec![b'0'; chunk_count];
            marks[0] = b'1';
            bytes.extend_from_slice(&marks);
            std::fs::write(dir.join(format!("{base}.parts")), &bytes).unwrap();
        }

        let url = format!("{}/blob", server.uri());
        let info = test_info(total, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, chunk_size);

        chunked_download(&info, &url, &dir, base, &downloaded, &copying, &cfg)
            .await
            .expect("resume should succeed");

        let out = std::fs::read(dir.join(base)).unwrap();
        assert_eq!(out, blob, "resumed file must be byte-identical");
        let ranges = log.lock().unwrap();
        assert!(
            ranges.iter().all(|(start, _)| *start != 0),
            "chunk 0 was pre-completed and must not be re-fetched; served ranges = {ranges:?}"
        );
        assert_eq!(
            ranges.len(),
            chunk_count - 1,
            "exactly the {} missing chunks should be fetched",
            chunk_count - 1
        );
        assert_eq!(
            downloaded.load(Ordering::Relaxed),
            total,
            "counter must include the seeded resume prefix plus the streamed remainder"
        );
        drop(ranges);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn chunked_download_errors_when_server_ignores_range() {
        // A server that returns 200 (full body) for a ranged request would corrupt the file if
        // written at a chunk offset — fetch_chunk must reject it instead.
        let blob = synthetic_blob(100 * 1024);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/full"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Accept-Ranges", "bytes")
                    .set_body_raw(blob.clone(), "application/octet-stream"),
            )
            .mount(&server)
            .await;
        let dir = unique_temp_dir("chunk-200");
        let url = format!("{}/full", server.uri());
        let info = test_info(blob.len() as u64, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(2, 32 * 1024);

        let err = chunked_download(&info, &url, &dir, "full.gguf", &downloaded, &copying, &cfg)
            .await
            .expect_err("a 200 (range ignored) must error, not corrupt the file");
        assert!(
            err.to_string().contains("ignored Range"),
            "unexpected error: {err:?}"
        );
        assert!(
            !dir.join("full.gguf").exists(),
            "no file should be published when the server ignores Range"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn fresh_part_discards_stale_all_done_manifest() {
        // Regression for the silent-corruption path: a prior run published the model but its
        // post-publish `remove_file(<base>.parts)` silently failed, leaving an all-`1` bitmap. The
        // model file is later removed; the next run creates a fresh zero-filled `.part`. Trusting
        // the stale manifest would skip every chunk and publish zeros — the length check passes and
        // there is no sha256 to catch it. The downloader must notice the brand-new part and refetch.
        let blob = synthetic_blob(300 * 1024);
        let chunk_size = 64 * 1024u64;
        let total = blob.len() as u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;
        let (server, log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-stale-manifest");
        let base = "model.gguf";

        // Pre-seed ONLY a stale, all-complete manifest — no `.part` on disk.
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            bytes.extend_from_slice(&vec![b'1'; chunk_count]);
            std::fs::write(dir.join(format!("{base}.parts")), &bytes).unwrap();
        }
        assert!(
            !dir.join(format!("{base}.part")).exists(),
            "precondition: no part file yet"
        );

        let url = format!("{}/blob", server.uri());
        let info = test_info(total, None); // no sha256 — re-fetching is the only safety net
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, chunk_size);

        chunked_download(&info, &url, &dir, base, &downloaded, &copying, &cfg)
            .await
            .expect(
                "a fresh part with a stale all-done manifest must re-download, not publish zeros",
            );

        let out = std::fs::read(dir.join(base)).unwrap();
        assert_eq!(
            out, blob,
            "published file must be the real bytes, not zeros"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            chunk_count,
            "every chunk must be re-fetched after the stale manifest is discarded"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn fresh_part_discards_stale_partial_manifest() {
        // Sibling of the all-done case: an interrupted download leaves a *partial* bitmap (some
        // chunks `1`, some `0`), then a user/cleanup tool reclaims the multi-GB `.part`. The next
        // run creates a fresh zero-filled `.part` while the partial bitmap survives. Trusting it
        // would skip the done-marked ranges, leaving zeros there — length passes and there is no
        // sha256 to catch it. The downloader must notice the brand-new part and refetch *every*
        // chunk, not just the ones still marked pending.
        let blob = synthetic_blob(300 * 1024);
        let chunk_size = 64 * 1024u64;
        let total = blob.len() as u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;
        let (server, log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-stale-partial-manifest");
        let base = "model.gguf";

        // Pre-seed a stale, PARTIALLY-complete manifest (first half done) — no `.part` on disk.
        let half = chunk_count / 2;
        assert!(
            half > 0 && half < chunk_count,
            "need a genuine mix of done/pending chunks"
        );
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            for i in 0..chunk_count {
                bytes.push(if i < half { b'1' } else { b'0' });
            }
            std::fs::write(dir.join(format!("{base}.parts")), &bytes).unwrap();
        }
        assert!(
            !dir.join(format!("{base}.part")).exists(),
            "precondition: no part file yet"
        );

        let url = format!("{}/blob", server.uri());
        let info = test_info(total, None); // no sha256 — re-fetching is the only safety net
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, chunk_size);

        chunked_download(&info, &url, &dir, base, &downloaded, &copying, &cfg)
            .await
            .expect(
                "a fresh part with a stale partial manifest must re-download, not publish zeros",
            );

        let out = std::fs::read(dir.join(base)).unwrap();
        assert_eq!(
            out, blob,
            "published file must be the real bytes, not zeros in the done-marked ranges"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            chunk_count,
            "every chunk must be re-fetched after the stale partial manifest is discarded"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn place_if_cached_short_circuits_when_dest_already_present() {
        // The cheap fast path the lock-retry re-checks after a backoff: when the file is already in
        // the clean layout (e.g. the lock holder finished and published it during our sleep),
        // `place_if_cached` returns true and counts its bytes — no network, no cache lookup needed.
        let dir = unique_temp_dir("place-if-cached-dest");
        let base = "model.gguf";
        let body = vec![7u8; 4096];
        std::fs::write(dir.join(base), &body).unwrap();
        let empty_cache = unique_temp_dir("place-if-cached-dest-cache");
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));

        let found = place_if_cached(
            &empty_cache,
            "org/repo",
            base,
            &dir,
            base,
            &downloaded,
            &copying,
        )
        .await
        .expect("place_if_cached must not error");

        assert!(
            found,
            "an already-present clean-layout file must short-circuit"
        );
        assert_eq!(
            downloaded.load(Ordering::Relaxed),
            body.len() as u64,
            "the on-disk size is counted exactly once"
        );
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(empty_cache);
    }

    #[tokio::test]
    async fn place_if_cached_returns_false_when_nothing_cached() {
        // Neither the clean layout nor the HF cache has the file → the caller must proceed to
        // actually download it (returns false, leaves the progress counter untouched).
        let dir = unique_temp_dir("place-if-cached-miss");
        let empty_cache = unique_temp_dir("place-if-cached-miss-cache");
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));

        let found = place_if_cached(
            &empty_cache,
            "org/repo",
            "model.gguf",
            &dir,
            "model.gguf",
            &downloaded,
            &copying,
        )
        .await
        .expect("place_if_cached must not error on a miss");

        assert!(!found, "a miss must report nothing was placed");
        assert_eq!(
            downloaded.load(Ordering::Relaxed),
            0,
            "a miss must not touch the progress counter"
        );
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(empty_cache);
    }

    #[tokio::test]
    async fn truncated_part_discards_stale_partial_manifest() {
        // Gap #69: an external cleanup tool *truncates* an existing multi-GB `.part` (shrinks it)
        // without deleting it, then the next run reopens it. `open_preallocated` re-grows it to
        // `total` with `set_len` — zero-filling the reclaimed tail — but because the file already
        // existed, the brand-new-part reset never fired. A header-matching partial bitmap that still
        // marks chunks `done` would skip them, leaving zeros there; the length check passes and there
        // is no sha256 to catch it. The downloader must notice the part can no longer back its
        // done-marks (on-disk length < total) and refetch *every* chunk.
        let blob = synthetic_blob(300 * 1024);
        let chunk_size = 64 * 1024u64;
        let total = blob.len() as u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;
        let (server, log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-truncated-partial-manifest");
        let base = "model.gguf";

        // Pre-seed a TRUNCATED `.part` (shorter than `total`, as a cleanup tool would leave it)
        // alongside a header-matching partial bitmap (first half done). A real interrupted download
        // preallocates the part to `total`; only truncation can leave it short.
        let half = chunk_count / 2;
        assert!(
            half > 0 && half < chunk_count,
            "need a genuine mix of done/pending chunks"
        );
        {
            let truncated_len = total / 4;
            assert!(
                truncated_len < total,
                "precondition: the part is shorter than total"
            );
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join(format!("{base}.part")))
                .unwrap();
            f.set_len(truncated_len).unwrap();
        }
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            for i in 0..chunk_count {
                bytes.push(if i < half { b'1' } else { b'0' });
            }
            std::fs::write(dir.join(format!("{base}.parts")), &bytes).unwrap();
        }

        let url = format!("{}/blob", server.uri());
        let info = test_info(total, None); // no sha256 — re-fetching is the only safety net
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, chunk_size);

        chunked_download(&info, &url, &dir, base, &downloaded, &copying, &cfg)
            .await
            .expect(
                "a truncated part with a stale partial manifest must re-download, not publish zeros",
            );

        let out = std::fs::read(dir.join(base)).unwrap();
        assert_eq!(
            out, blob,
            "published file must be the real bytes, not zeros in the done-marked ranges"
        );
        assert_eq!(
            log.lock().unwrap().len(),
            chunk_count,
            "every chunk must be re-fetched after a stale manifest over a truncated part is discarded"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn oversized_part_discards_stale_manifest() {
        // Defensive sibling of the truncated case (gap #69, broadened to any size mismatch): external
        // corruption leaves an existing `.part` *longer* than `total` (e.g. junk appended). `set_len`
        // shrinks it back to `total`, but those bytes can't be trusted to back a header-matching
        // bitmap's done-marks — a legitimate resume is always preallocated to exactly `total`, never
        // larger. `open_preallocated` reports any length != total as unbacked, so the stale manifest
        // is discarded and every chunk refetched.
        let blob = synthetic_blob(300 * 1024);
        let chunk_size = 64 * 1024u64;
        let total = blob.len() as u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;
        let (server, log) = mount_range_server(blob.clone()).await;
        let dir = unique_temp_dir("chunk-oversized-manifest");
        let base = "model.gguf";

        {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join(format!("{base}.part")))
                .unwrap();
            f.set_len(total * 2).unwrap(); // larger than total — never a legitimate resume state
        }
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            bytes.extend_from_slice(&vec![b'1'; chunk_count]); // stale all-done
            std::fs::write(dir.join(format!("{base}.parts")), &bytes).unwrap();
        }

        let url = format!("{}/blob", server.uri());
        let info = test_info(total, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(4, chunk_size);

        chunked_download(&info, &url, &dir, base, &downloaded, &copying, &cfg)
            .await
            .expect("an oversized part with a stale manifest must re-download, not publish junk");

        let out = std::fs::read(dir.join(base)).unwrap();
        assert_eq!(out, blob, "published file must be the real bytes");
        assert_eq!(
            log.lock().unwrap().len(),
            chunk_count,
            "every chunk must be re-fetched after a stale manifest over an oversized part is discarded"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn exhausted_transient_is_not_reported_as_ignored_range() {
        // A chunk that 403s past its retry budget is an auth/throttle failure, not a server that
        // ignored Range. Both terminate at the same return; the message must still tell them apart
        // so a log reader doesn't chase a Range-support bug that doesn't exist.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/denied"))
            .respond_with(ResponseTemplate::new(403).set_body_string("denied"))
            .mount(&server)
            .await;
        let dir = unique_temp_dir("chunk-403-forever");
        let url = format!("{}/denied", server.uri());
        let total = 20 * 1024u64;
        let info = test_info(total, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(1, total); // one chunk; it exhausts its retries and fails

        let err = chunked_download(
            &info,
            &url,
            &dir,
            "denied.gguf",
            &downloaded,
            &copying,
            &cfg,
        )
        .await
        .expect_err("a chunk that 403s past its retry budget must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("failed after") && msg.contains("retries"),
            "expected a retry-exhaustion message, got: {msg}"
        );
        assert!(
            !msg.contains("ignored Range"),
            "a 403 is not an ignored-Range condition: {msg}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn manifest_load_or_init_distinguishes_missing_valid_and_mismatched() {
        // Guards the load/init refactor: a missing manifest starts all-pending; a valid one is
        // loaded verbatim; a header that belongs to a different download is re-initialised. (The
        // separate present-but-unreadable → propagate-error branch can't be injected without a real
        // sharing violation, so it is covered by inspection rather than a unit test.)
        let dir = unique_temp_dir("manifest-load");
        let path = dir.join("m.parts");
        let total = 300 * 1024u64;
        let chunk_size = 64 * 1024u64;
        let chunk_count = total.div_ceil(chunk_size) as usize;

        // Missing → fresh, nothing complete.
        let fresh = Manifest::load_or_init_sync(&path, total, chunk_size, chunk_count).unwrap();
        assert_eq!(fresh.pending_indices().len(), chunk_count);
        assert!(path.exists(), "init must persist a bitmap");

        // Valid existing (chunk 0 marked) → loaded verbatim.
        {
            let header = Manifest::header_for(total, chunk_size, chunk_count);
            let mut bytes = header.into_bytes();
            let mut marks = vec![b'0'; chunk_count];
            marks[0] = b'1';
            bytes.extend_from_slice(&marks);
            std::fs::write(&path, &bytes).unwrap();
        }
        let loaded = Manifest::load_or_init_sync(&path, total, chunk_size, chunk_count).unwrap();
        assert_eq!(
            loaded.pending_indices().len(),
            chunk_count - 1,
            "a valid bitmap must be loaded, not clobbered"
        );

        // Header for a *different* download (different total) → re-initialised all-pending.
        let other = Manifest::load_or_init_sync(&path, total + 1, chunk_size, chunk_count).unwrap();
        assert_eq!(
            other.pending_indices().len(),
            chunk_count,
            "a mismatched header must be treated as stale and re-initialised"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn chunked_download_fails_fast_on_stuck_chunk() {
        // The chunk source sends 206 headers then hangs; the per-read timeout must fail the chunk
        // quickly (the per-chunk backstop to the aggregate-progress stall watchdog).
        let total = 64 * 1024u64;
        let addr = spawn_hang_after_206(total).await;
        let dir = unique_temp_dir("chunk-stall");
        let url = format!("http://{addr}/x");
        let info = test_info(total, None);
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = ChunkConfig {
            conns: 2,
            chunk_size: total,
            connect_timeout: Duration::from_millis(500),
            read_timeout: Duration::from_millis(80),
        };

        let started = std::time::Instant::now();
        let err = chunked_download(&info, &url, &dir, "x.gguf", &downloaded, &copying, &cfg)
            .await
            .expect_err("a stuck chunk must error, not hang");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(3),
            "stuck chunk should fail fast via the read timeout, took {elapsed:?}"
        );
        let _ = err;
        assert!(
            dir.join("x.gguf.part").exists(),
            "the .part must remain for resume after a transient stall"
        );
        assert!(
            dir.join("x.gguf.parts").exists(),
            "the resume bitmap must remain for resume after a transient stall"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn integrity_accepts_matching_sha256_and_rejects_a_wrong_one() {
        let blob = synthetic_blob(120 * 1024);
        let (server, _log) = mount_range_server(blob.clone()).await;
        let url = format!("{}/blob", server.uri());

        // Matching hash → published.
        let good = test_info(blob.len() as u64, Some(sha256_hex(&blob)));
        let dir_ok = unique_temp_dir("chunk-sha-ok");
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let cfg = test_cfg(3, 32 * 1024);
        chunked_download(&good, &url, &dir_ok, "ok.gguf", &downloaded, &copying, &cfg)
            .await
            .expect("matching sha256 must verify");
        assert!(
            dir_ok.join("ok.gguf").exists(),
            "verified file is published"
        );
        let _ = std::fs::remove_dir_all(dir_ok);

        // Wrong hash → rejected, partial discarded so a retry is clean.
        let bad = test_info(blob.len() as u64, Some("0".repeat(64)));
        let dir_bad = unique_temp_dir("chunk-sha-bad");
        let downloaded = Arc::new(AtomicU64::new(0));
        let copying = Arc::new(AtomicBool::new(false));
        let err = chunked_download(
            &bad,
            &url,
            &dir_bad,
            "bad.gguf",
            &downloaded,
            &copying,
            &cfg,
        )
        .await
        .expect_err("a wrong sha256 must fail the download");
        assert!(
            err.to_string().contains("sha256 mismatch"),
            "unexpected error: {err:?}"
        );
        assert!(
            !dir_bad.join("bad.gguf").exists(),
            "a hash-mismatched file must never be published"
        );
        assert!(
            !dir_bad.join("bad.gguf.part").exists(),
            "a hash-mismatched partial must be discarded for a clean retry"
        );
        let _ = std::fs::remove_dir_all(dir_bad);
    }

    /// A local TCP server that, for any request, writes `206` headers promising `total` bytes and
    /// then hangs without sending a body — the deterministic stand-in for a CDN that accepts the
    /// socket then goes silent mid-transfer.
    async fn spawn_hang_after_206(total: u64) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let headers = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        total - 1,
                        total,
                        total
                    );
                    let _ = sock.write_all(headers.as_bytes()).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                });
            }
        });
        addr
    }
}
