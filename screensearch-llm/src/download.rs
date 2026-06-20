//! Model and binary download utilities for Ministral-3B and llama-server

use crate::error::{LlmError, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// URL to download the Ministral-3B GGUF model from
pub const MODEL_URL: &str = "https://leophir.com/models/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Model filename
pub const MODEL_FILENAME: &str = "Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Approximate model size in bytes (~2.15 GB)
pub const MODEL_SIZE_BYTES: u64 = 2_150_000_000;

// ============================================================
// llama-server binary download
// ============================================================

/// llama-server binary filename
#[cfg(windows)]
pub const LLAMA_SERVER_FILENAME: &str = "llama-server.exe";

#[cfg(not(windows))]
pub const LLAMA_SERVER_FILENAME: &str = "llama-server";

/// Approximate llama-server binary size in bytes (~37MB ZIP for the Vulkan
/// Windows build as of b9728)
pub const LLAMA_SERVER_SIZE_BYTES: u64 = 37_000_000;

/// llama.cpp releases base URL (moved from ggerganov to ggml-org)
#[allow(dead_code)]
const LLAMA_CPP_RELEASES_URL: &str = "https://github.com/ggml-org/llama.cpp/releases";

/// Current pinned llama.cpp build. Bumped from b7562 → b9728 so the server can
/// load recent model architectures (e.g. `gemma4`, `qwen35`); older builds fail
/// with "unknown model architecture". Recorded next to the downloaded binary so
/// an existing install re-downloads when this pin changes (see
/// `llama_server_up_to_date`).
pub const LLAMA_VERSION: &str = "b9728";

/// Get the latest llama-server download URL for the current platform
#[cfg(all(windows, target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // Vulkan GPU-accelerated Windows build (works on NVIDIA, AMD, Intel GPUs)
    // Falls back to CPU automatically if no compatible GPU is available
    // Vulkan is pre-installed with modern GPU drivers, no additional setup needed
    "https://github.com/ggml-org/llama.cpp/releases/download/b9728/llama-b9728-bin-win-vulkan-x64.zip"
}

// Note: ScreenSearch is a Windows-only application; the non-Windows URLs below
// are kept current for completeness. Recent llama.cpp releases ship these as
// .tar.gz (the extractor here handles .zip), so non-Windows targets are not a
// supported runtime path.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // Linux x64 build (CPU)
    "https://github.com/ggml-org/llama.cpp/releases/download/b9728/llama-b9728-bin-ubuntu-x64.tar.gz"
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn get_llama_server_url() -> &'static str {
    // macOS ARM64 build (Apple Silicon)
    "https://github.com/ggml-org/llama.cpp/releases/download/b9728/llama-b9728-bin-macos-arm64.tar.gz"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // macOS x64 build (Intel)
    "https://github.com/ggml-org/llama.cpp/releases/download/b9728/llama-b9728-bin-macos-x64.tar.gz"
}

// Fallback for unsupported platforms
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
)))]
pub fn get_llama_server_url() -> &'static str {
    ""
}

/// Progress information for model download
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download speed in bytes per second
    pub speed_bps: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: u64,
}

impl DownloadProgress {
    /// Get progress as a percentage (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.bytes_downloaded as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Get the bin directory path (for llama-server and other binaries)
///
/// Priority order:
/// 1. Same directory as screensearch.exe (installed/portable)
/// 2. "bin" directory next to the executable
/// 3. User's app data directory (standard install)
pub fn get_bin_dir() -> PathBuf {
    // 1. Check same directory as executable (bundled with installer)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let llama_server = exe_dir.join(LLAMA_SERVER_FILENAME);
            if llama_server.exists() {
                debug!("Using bin directory same as exe: {:?}", exe_dir);
                return exe_dir.to_path_buf();
            }

            // 2. Check "bin" subdirectory next to executable
            let exe_bin = exe_dir.join("bin");
            if exe_bin.exists() {
                debug!("Using bin directory next to exe: {:?}", exe_bin);
                return exe_bin;
            }
        }
    }

    // 3. Use user's app data directory
    if let Some(data_dir) = dirs::data_local_dir() {
        let app_bin = data_dir.join("ScreenSearch").join("bin");
        debug!("Using bin directory in AppData: {:?}", app_bin);
        return app_bin;
    }

    // 4. Default fallback
    debug!("Using default bin directory: bin/");
    PathBuf::from("bin")
}

/// Get the full path to the llama-server binary
pub fn get_llama_server_path() -> PathBuf {
    get_bin_dir().join(LLAMA_SERVER_FILENAME)
}

/// Check if llama-server binary exists
pub fn llama_server_exists(bin_dir: &Path) -> bool {
    let server_path = bin_dir.join(LLAMA_SERVER_FILENAME);
    server_path.exists()
}

/// Path to the marker recording which llama.cpp build is installed in `bin_dir`.
fn llama_version_marker_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join("llama-version.txt")
}

/// The llama.cpp build recorded in `bin_dir`, if any.
fn installed_llama_version(bin_dir: &Path) -> Option<String> {
    std::fs::read_to_string(llama_version_marker_path(bin_dir))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Whether the installed llama-server binary is present AND matches the pinned
/// `LLAMA_VERSION`. A binary left over from an older pin is treated as out of
/// date so the app re-downloads a build that supports current model
/// architectures (e.g. `gemma4`, `qwen35`).
pub fn llama_server_up_to_date(bin_dir: &Path) -> bool {
    llama_server_exists(bin_dir)
        && installed_llama_version(bin_dir).as_deref() == Some(LLAMA_VERSION)
}

/// Check if llama-server download is needed (missing or version mismatch)
pub fn needs_llama_server_download() -> bool {
    let bin_dir = get_bin_dir();
    !llama_server_up_to_date(&bin_dir)
}

/// Get the models directory path
///
/// Priority order:
/// 1. "models" in current working directory (portable/bundled)
/// 2. "models" next to the executable (portable install)
/// 3. User's app data directory (standard Windows install)
/// 4. Default to "models" in current directory (will be created)
pub fn get_models_dir() -> PathBuf {
    // 1. Check "models" in current working directory
    let cwd_models = PathBuf::from("models");
    if cwd_models.exists() {
        debug!("Using models directory from CWD: {:?}", cwd_models);
        return cwd_models;
    }

    // 2. Check "models" next to the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_models = exe_dir.join("models");
            if exe_models.exists() {
                debug!("Using models directory next to exe: {:?}", exe_models);
                return exe_models;
            }
        }
    }

    // 3. Use user's app data directory
    if let Some(data_dir) = dirs::data_local_dir() {
        let app_models = data_dir.join("ScreenSearch").join("models");
        debug!("Using models directory in AppData: {:?}", app_models);
        return app_models;
    }

    // 4. Default fallback
    debug!("Using default models directory: models/");
    PathBuf::from("models")
}

/// Get the full path to the default (downloadable) model file.
pub fn get_model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILENAME)
}

/// Candidate directories scanned for user-provided GGUF models, in priority
/// order. `.models/` (next to the binary or in the working directory) is where
/// users drop their own models; the standard `models/` and AppData locations
/// are also searched so a downloaded default is picked up too.
pub fn model_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    fn push(dir: PathBuf, list: &mut Vec<PathBuf>) {
        if !list.contains(&dir) {
            list.push(dir);
        }
    }

    // Working directory (dev runs from the repo root, where `.models/` lives).
    push(PathBuf::from(".models"), &mut dirs);
    push(PathBuf::from("models"), &mut dirs);

    // Next to the executable (portable / installed layout).
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            push(exe_dir.join(".models"), &mut dirs);
            push(exe_dir.join("models"), &mut dirs);
        }
    }

    // Standard per-user data directory.
    if let Some(data_dir) = dirs::data_local_dir() {
        push(data_dir.join("ScreenSearch").join("models"), &mut dirs);
    }

    dirs
}

/// Discover every `*.gguf` file across the model search directories.
///
/// Results are ordered by directory priority, then by filename within a
/// directory, so the ordering is stable across runs.
pub fn discover_local_models() -> Vec<PathBuf> {
    let mut models: Vec<PathBuf> = Vec::new();
    for dir in model_search_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut in_dir: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
                    && is_loadable_model_gguf(path)
            })
            // Canonicalize so the same physical file reached via different
            // search dirs (e.g. relative `.models` and an absolute exe-dir
            // path) is not reported twice.
            .map(|path| path.canonicalize().unwrap_or(path))
            .collect();
        in_dir.sort();
        for path in in_dir {
            if !models.contains(&path) {
                models.push(path);
            }
        }
    }
    models
}

/// Whether a local GGUF model is available for the answer-generation LLM —
/// either a user-provided model discovered in `.models/` (etc.) or the
/// downloadable default model.
pub fn local_model_available() -> bool {
    !discover_local_models().is_empty() || model_exists(&get_models_dir())
}

/// Desirability of a discovered GGUF as the *answer-generation* model. Mirrors
/// the vision resolver's intent: a vanilla `instruct` build is preferred, while
/// chain-of-thought (`thinking`) builds and third-party agent (`action`)
/// fine-tunes are strongly penalised so they are never auto-selected just
/// because they sort first; quantisation desirability breaks remaining ties.
/// (`thinking`/`action` are matched as whole tokens to avoid tripping on
/// substrings such as `extraction`.)
fn answer_model_score(stem_lower: &str) -> i64 {
    let has_token = |tok: &str| {
        stem_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|t| t == tok)
    };
    let mut score: i64 = 0;
    if !has_token("thinking") {
        score += 1000;
    }
    if !has_token("action") {
        score += 1000;
    }
    if stem_lower.contains("instruct") {
        score += 200;
    }
    score += quant_desirability(stem_lower) as i64;
    score
}

/// Pick the best answer-generation model from discovered candidates, preferring
/// the earliest on ties (stable discovery order).
fn pick_answer_model(models: &[PathBuf]) -> Option<PathBuf> {
    let mut best: Option<(&PathBuf, i64)> = None;
    for p in models {
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let score = answer_model_score(&stem);
        // Strictly-greater so the first candidate on a tie wins.
        if best.map(|(_, b)| score > b).unwrap_or(true) {
            best = Some((p, score));
        }
    }
    best.map(|(p, _)| p.clone())
}

/// The GGUF model the LLM server should use by default: the best-scoring
/// discovered user-provided model (see [`answer_model_score`]), falling back to
/// the downloadable default path (which may or may not exist yet).
pub fn resolve_model_path() -> PathBuf {
    if let Some(found) = pick_answer_model(&discover_local_models()) {
        debug!("Using discovered GGUF model: {:?}", found);
        return found;
    }
    get_model_path(&get_models_dir())
}

// ============================================================
// Vision (multimodal projector) discovery
// ============================================================

/// Whether `token` looks like a model size marker such as `4b`, `12b`, `27b`,
/// or `e4b` / `e2b` (an optional single-letter prefix, one or more digits, then
/// a trailing `b`). Used to pair a model with the right `mmproj` projector when
/// several projectors of different sizes sit in the same directory.
fn is_size_token(token: &str) -> bool {
    if token.len() < 2 || token.len() > 4 || !token.ends_with('b') {
        return false;
    }
    let core = &token[..token.len() - 1];
    // Drop an optional single leading letter (e.g. the `e` in `e4b`).
    let digits = core
        .strip_prefix(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(core);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// Extract the first size signature (e.g. `e4b`, `12b`) from a (lowercased)
/// filename stem, if any. Tokens are split on non-alphanumeric characters.
fn size_signature(stem_lower: &str) -> Option<String> {
    stem_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|tok| is_size_token(tok))
        .map(|tok| tok.to_string())
}

/// Count of shared tokens between two (lowercased) stems.
fn token_overlap(a_lower: &str, b_lower: &str) -> usize {
    let a: std::collections::HashSet<&str> = a_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    b_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .filter(|t| a.contains(t))
        .count()
}

/// Minimum size for a file to be treated as a loadable *primary* model. Filters
/// out helper GGUFs that live beside real models (e.g. a ~59 MB multi-token
/// prediction head, `mtp-*.gguf`) so they are never loaded as the generation or
/// vision model.
const MODEL_MIN_SIZE_BYTES: u64 = 500_000_000;

/// Whether a GGUF path is a loadable *primary* model — not a multimodal
/// projector, not a non-standalone helper head (`mtp-*`), and large enough to be
/// a real model. Projectors are discovered and paired separately via
/// [`resolve_mmproj_for`].
fn is_loadable_model_gguf(path: &Path) -> bool {
    if is_mmproj_file(path) {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
        .unwrap_or_default();
    // `mtp-*` is a multi-token-prediction head shipped beside Gemma 4 models,
    // not a model that can be loaded on its own.
    if name.starts_with("mtp-") {
        return false;
    }
    // Embedding models (e.g. `*-Embedding-*`, EmbeddingGemma) produce vectors,
    // not chat completions. They must never be selected as the answer-generation
    // model or a vision base, even though they are sizeable standalone GGUFs.
    if name.contains("embed") {
        return false;
    }
    match std::fs::metadata(path) {
        Ok(meta) => meta.len() >= MODEL_MIN_SIZE_BYTES,
        Err(_) => false,
    }
}

/// The quantization level (the digit in a `q2`..`q8` token) of a lowercased
/// filename stem, if present. E.g. `...-q4_k_xl` and `...q4_0...` both → 4.
fn quant_level(stem_lower: &str) -> Option<u8> {
    stem_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find_map(|tok| {
            let b = tok.as_bytes();
            if b.len() == 2 && b[0] == b'q' && b[1].is_ascii_digit() {
                Some(b[1] - b'0')
            } else {
                None
            }
        })
}

/// Desirability score for a model's quantization for continuous on-device use:
/// Q4 is the sweet spot (quality vs. speed/VRAM); higher quants trade speed for
/// marginal quality; Q2/Q3 are penalised so a low-quality quant is only picked
/// when nothing better sits beside it. Unknown/unquantized → neutral.
fn quant_desirability(stem_lower: &str) -> u32 {
    match quant_level(stem_lower) {
        Some(4) => 100,
        Some(5) => 90,
        Some(6) => 80,
        Some(8) => 70,
        Some(7) => 60,
        Some(3) => 40,
        Some(2) => 30,
        Some(0 | 1) => 25,
        Some(_) => 50,
        None => 50,
    }
}

/// From vision-model candidates (in stable discovery order), pick the one with
/// the best quantization, preferring the earliest on ties.
fn pick_best_quant<'a>(
    candidates: impl Iterator<Item = &'a (PathBuf, PathBuf)>,
) -> Option<&'a (PathBuf, PathBuf)> {
    let mut best: Option<(&'a (PathBuf, PathBuf), u32)> = None;
    for cand in candidates {
        let score = cand
            .0
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| quant_desirability(&s.to_ascii_lowercase()))
            .unwrap_or(50);
        // Strictly-greater so the first candidate on a tie wins (stable order).
        if best.map(|(_, b)| score > b).unwrap_or(true) {
            best = Some((cand, score));
        }
    }
    best.map(|(c, _)| c)
}

/// Whether a GGUF filename is a multimodal projector (its name contains
/// `mmproj`, the llama.cpp convention).
fn is_mmproj_file(path: &Path) -> bool {
    let is_gguf = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    let named = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().contains("mmproj"));
    path.is_file() && is_gguf && named
}

/// Find the multimodal projector (`mmproj` GGUF) that pairs with `model_path`,
/// if one sits beside it.
///
/// llama.cpp needs `--mmproj <projector>` to enable vision for models like
/// Gemma 4. A directory may hold several projectors (e.g. one for E4B and one
/// for 12B), so we match on the model's size signature; if the model has a
/// size signature but no projector matches it, we return `None` rather than
/// guessing (a text-only model like Qwen3.5 must not be paired with a Gemma
/// projector).
pub fn resolve_mmproj_for(model_path: &Path) -> Option<PathBuf> {
    // A projector is never its own projector.
    if is_mmproj_file(model_path) {
        return None;
    }
    let dir = model_path.parent()?;
    let model_stem = model_path.file_stem()?.to_str()?.to_ascii_lowercase();

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_mmproj_file(p))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort();

    if let Some(model_sig) = size_signature(&model_stem) {
        // Require the projector to share the model's size signature.
        return candidates.into_iter().find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| size_signature(&s.to_ascii_lowercase()))
                .as_deref()
                == Some(model_sig.as_str())
        });
    }

    // Model has no size signature: a single generic projector pairs cleanly;
    // otherwise fall back to the best token overlap.
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    candidates.into_iter().max_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| token_overlap(&model_stem, &s.to_ascii_lowercase()))
            .unwrap_or(0)
    })
}

/// Discover every `(model, mmproj)` pair among the local GGUF models — i.e.
/// every vision-capable model that has a matching projector beside it.
///
/// Ordering follows [`discover_local_models`] (directory priority, then
/// filename), so it is stable across runs.
pub fn discover_vision_models() -> Vec<(PathBuf, PathBuf)> {
    discover_local_models()
        .into_iter()
        .filter_map(|model| resolve_mmproj_for(&model).map(|proj| (model, proj)))
        .collect()
}

/// Resolve the vision model + projector to run for the unified local server.
///
/// Selection order (within each tier, the best quantization wins — see
/// [`quant_desirability`] — so a Q4 is chosen over a Q2 sitting beside it):
/// 1. Discovered vision models whose filename matches `preferred` (the
///    `vision_model` setting), in either direction (substring). When `preferred`
///    does not itself ask for a `thinking` or `action`/agent variant, those are
///    excluded from a generic match — so the default `Qwen3-VL-4B-Instruct`
///    resolves to the vanilla instruct model rather than a slow `*-thinking`
///    build or a third-party `*-action` fine-tune that merely contains the same
///    substring.
/// 2. Gemma-4 **E4B** models — the previous default (still valid if present).
/// 3. The first vision-capable model discovered.
///
/// Whether a vision-model filename `stem_lower` satisfies the `vision_model`
/// preference `pref` (both already lowercased). The base rule is substring
/// containment in either direction; additionally, a generic preference does NOT
/// match `thinking` or `action`/agent variants unless it explicitly names them.
/// This keeps the default `Qwen3-VL-4B-Instruct` resolving to the vanilla
/// instruct build rather than a slower `*-thinking` model or a third-party
/// `*-action` fine-tune that merely contains the same substring, while a user
/// who deliberately sets one of those as their preference still gets it.
fn pref_matches_vision_stem(stem_lower: &str, pref: &str) -> bool {
    let hit = stem_lower.contains(pref) || pref.contains(stem_lower);
    // Match `thinking` / `action` only as whole tokens (split on non-alphanumeric)
    // so a generic preference is not tripped by substrings such as `extraction`,
    // `interaction`, or `transaction` in an unrelated model name.
    let has_token = |s: &str, tok: &str| {
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|t| t == tok)
    };
    let allow_thinking = has_token(pref, "thinking");
    let allow_action = has_token(pref, "action");
    hit && (allow_thinking || !has_token(stem_lower, "thinking"))
        && (allow_action || !has_token(stem_lower, "action"))
}

/// Returns `None` when no local model has a projector beside it.
pub fn resolve_vision_model(preferred: &str) -> Option<(PathBuf, PathBuf)> {
    let models = discover_vision_models();
    if models.is_empty() {
        return None;
    }

    let stem_lower = |m: &Path| {
        m.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
    };

    let pref = preferred.trim().to_ascii_lowercase();
    if !pref.is_empty() {
        let matches = models.iter().filter(|(m, _)| {
            stem_lower(m)
                .map(|s| pref_matches_vision_stem(&s, &pref))
                .unwrap_or(false)
        });
        if let Some(found) = pick_best_quant(matches) {
            return Some(found.clone());
        }
    }

    let e4b = models
        .iter()
        .filter(|(m, _)| stem_lower(m).map(|s| s.contains("e4b")).unwrap_or(false));
    if let Some(found) = pick_best_quant(e4b) {
        return Some(found.clone());
    }

    models.into_iter().next()
}

/// Check if the model exists and is valid
pub fn model_exists(models_dir: &Path) -> bool {
    let model_path = get_model_path(models_dir);
    if !model_path.exists() {
        return false;
    }

    // Verify file size is reasonable (at least 1GB for a valid GGUF model)
    match std::fs::metadata(&model_path) {
        Ok(meta) => {
            let size = meta.len();
            if size < 1_000_000_000 {
                warn!(
                    "Model file exists but seems too small ({} bytes). May be corrupted.",
                    size
                );
                return false;
            }
            true
        }
        Err(e) => {
            warn!("Failed to check model file: {}", e);
            false
        }
    }
}

/// Check if model download is needed
pub fn needs_download(models_dir: &Path) -> bool {
    !model_exists(models_dir)
}

/// Download the model from the configured URL
///
/// This is a simple download without progress reporting.
/// For progress updates, use `download_model_with_progress`.
pub async fn download_model(models_dir: &Path) -> Result<PathBuf> {
    download_model_with_progress(models_dir, None).await
}

/// Download the model with optional progress reporting
///
/// # Arguments
/// * `models_dir` - Directory to store the model
/// * `progress_tx` - Optional channel to send progress updates
///
/// # Returns
/// Path to the downloaded model file
pub async fn download_model_with_progress(
    models_dir: &Path,
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    // Create models directory if it doesn't exist
    tokio::fs::create_dir_all(models_dir).await?;

    let model_path = get_model_path(models_dir);

    // Check if already downloaded
    if model_exists(models_dir) {
        info!("Model already downloaded at {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading Ministral-3B model from: {}", MODEL_URL);
    info!("Destination: {:?}", model_path);

    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600)) // 1 hour timeout for large file
        .build()
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to create HTTP client: {}", e)))?;

    // Start download
    let response = client
        .get(MODEL_URL)
        .send()
        .await
        .map_err(|e| LlmError::DownloadFailed(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(LlmError::DownloadFailed(format!(
            "HTTP error: {} - {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    let total_size = response.content_length().unwrap_or(MODEL_SIZE_BYTES);

    // Create progress bar for terminal
    let progress_bar = ProgressBar::new(total_size);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("Invalid progress template")
            .progress_chars("#>-"),
    );

    // Download to temp file first (atomic write)
    let temp_path = model_path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&temp_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| LlmError::DownloadFailed(format!("Failed to read chunk: {}", e)))?;

        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        progress_bar.set_position(downloaded);

        // Send progress update if channel provided
        if let Some(ref tx) = progress_tx {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (downloaded as f64 / elapsed) as u64
            } else {
                0
            };
            let remaining = total_size.saturating_sub(downloaded);
            let eta = remaining.checked_div(speed).unwrap_or(0);

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: total_size,
                speed_bps: speed,
                eta_seconds: eta,
            };

            // Non-blocking send (drop if receiver is slow)
            let _ = tx.try_send(progress);
        }
    }

    file.flush().await?;
    drop(file);

    // Rename temp file to final name (atomic on most filesystems)
    tokio::fs::rename(&temp_path, &model_path).await?;

    progress_bar.finish_with_message("Download complete!");
    info!("Model downloaded successfully to {:?}", model_path);

    // Verify the downloaded file
    if !model_exists(models_dir) {
        return Err(LlmError::DownloadFailed(
            "Downloaded file verification failed".to_string(),
        ));
    }

    Ok(model_path)
}

/// Delete the downloaded model
#[allow(dead_code)]
pub async fn delete_model(models_dir: &Path) -> Result<()> {
    let model_path = get_model_path(models_dir);
    if model_path.exists() {
        tokio::fs::remove_file(&model_path).await?;
        info!("Deleted model at {:?}", model_path);
    }
    Ok(())
}

// ============================================================
// llama-server Binary Download
// ============================================================

/// Download the llama-server binary
///
/// Downloads from llama.cpp GitHub releases and extracts the server binary.
pub async fn download_llama_server() -> Result<PathBuf> {
    download_llama_server_with_progress(None).await
}

/// Download the llama-server binary with optional progress reporting
pub async fn download_llama_server_with_progress(
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    let bin_dir = get_bin_dir();

    // Check if already downloaded AND matching the pinned version. A binary from
    // an older pin is re-downloaded so it can load current model architectures.
    if llama_server_up_to_date(&bin_dir) {
        let server_path = bin_dir.join(LLAMA_SERVER_FILENAME);
        info!(
            "llama-server {} already downloaded at {:?}",
            LLAMA_VERSION, server_path
        );
        return Ok(server_path);
    }
    if llama_server_exists(&bin_dir) {
        info!(
            "Existing llama-server is from a different build (need {}); re-downloading",
            LLAMA_VERSION
        );
    }

    // Get download URL for current platform
    let download_url = get_llama_server_url();
    if download_url.is_empty() {
        return Err(LlmError::DownloadFailed(
            "llama-server download not available for this platform".to_string(),
        ));
    }

    info!("Downloading llama-server from: {}", download_url);
    info!("Destination: {:?}", bin_dir);

    // Create bin directory if it doesn't exist
    tokio::fs::create_dir_all(&bin_dir).await?;

    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout
        .build()
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to create HTTP client: {}", e)))?;

    // Start download
    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| LlmError::DownloadFailed(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(LlmError::DownloadFailed(format!(
            "HTTP error: {} - {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    let total_size = response
        .content_length()
        .unwrap_or(LLAMA_SERVER_SIZE_BYTES * 5); // ZIP is larger

    // Create progress bar for terminal
    let progress_bar = ProgressBar::new(total_size);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("Invalid progress template")
            .progress_chars("#>-"),
    );

    // Download to temp file
    let temp_zip_path = bin_dir.join("llama-server.zip.tmp");
    let mut file = tokio::fs::File::create(&temp_zip_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| LlmError::DownloadFailed(format!("Failed to read chunk: {}", e)))?;

        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        progress_bar.set_position(downloaded);

        // Send progress update if channel provided
        if let Some(ref tx) = progress_tx {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (downloaded as f64 / elapsed) as u64
            } else {
                0
            };
            let remaining = total_size.saturating_sub(downloaded);
            let eta = remaining.checked_div(speed).unwrap_or(0);

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: total_size,
                speed_bps: speed,
                eta_seconds: eta,
            };

            let _ = tx.try_send(progress);
        }
    }

    file.flush().await?;
    drop(file);

    progress_bar.finish_with_message("Download complete! Extracting...");

    // Extract the ZIP file
    let zip_path = bin_dir.join("llama-server.zip");
    tokio::fs::rename(&temp_zip_path, &zip_path).await?;

    // Extract synchronously (zip crate isn't async)
    let bin_dir_clone = bin_dir.clone();
    let server_path =
        tokio::task::spawn_blocking(move || extract_llama_server(&zip_path, &bin_dir_clone))
            .await
            .map_err(|e| LlmError::DownloadFailed(format!("Extract task failed: {}", e)))??;

    // Clean up ZIP file
    let _ = tokio::fs::remove_file(bin_dir.join("llama-server.zip")).await;

    info!("llama-server extracted to {:?}", server_path);

    // Verify extraction
    if !llama_server_exists(&bin_dir) {
        return Err(LlmError::DownloadFailed(
            "Extraction failed - llama-server not found".to_string(),
        ));
    }

    // Record which build was installed so a future version bump triggers a
    // re-download (see llama_server_up_to_date).
    if let Err(e) = std::fs::write(llama_version_marker_path(&bin_dir), LLAMA_VERSION) {
        warn!("Failed to write llama-server version marker: {}", e);
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&server_path, permissions)
            .map_err(|e| LlmError::DownloadFailed(format!("Failed to set permissions: {}", e)))?;
    }

    Ok(server_path)
}

/// Extract llama-server and all dependencies from a ZIP archive
///
/// Extracts the llama-server executable plus all shared libraries (.dll, .so, .dylib)
/// that are required for the server to run.
fn extract_llama_server(zip_path: &Path, bin_dir: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to open ZIP: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to read ZIP: {}", e)))?;

    let target_filename = LLAMA_SERVER_FILENAME;
    let mut server_path: Option<PathBuf> = None;
    let mut extracted_count = 0;

    info!("Extracting {} files from ZIP archive...", archive.len());

    // Extract ALL relevant files from the archive (executables and shared libraries)
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| LlmError::DownloadFailed(format!("Failed to read ZIP entry: {}", e)))?;

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_string();

        // Get just the filename (strip any subdirectory path like "build/bin/")
        let filename = match Path::new(&entry_name).file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        if filename.is_empty() {
            continue;
        }

        // Check if this is a file we need to extract:
        // - Executables (.exe on Windows, no extension on Unix)
        // - Shared libraries (.dll on Windows, .so on Linux, .dylib on macOS)
        let is_executable = filename == target_filename
            || filename == "llama-server"
            || filename == "llama-server.exe"
            || filename.ends_with(".exe");

        let is_shared_library = filename.ends_with(".dll")
            || filename.ends_with(".so")
            || filename.contains(".so.")  // e.g., libfoo.so.1
            || filename.ends_with(".dylib");

        if is_executable || is_shared_library {
            let output_path = bin_dir.join(&filename);
            debug!("Extracting: {} -> {:?}", entry_name, output_path);

            let mut output_file = std::fs::File::create(&output_path).map_err(|e| {
                LlmError::DownloadFailed(format!("Failed to create {}: {}", filename, e))
            })?;

            std::io::copy(&mut entry, &mut output_file).map_err(|e| {
                LlmError::DownloadFailed(format!("Failed to extract {}: {}", filename, e))
            })?;

            extracted_count += 1;

            // Track the server executable path
            if filename == target_filename
                || filename == "llama-server"
                || filename == "llama-server.exe"
            {
                server_path = Some(output_path.clone());
                info!("Found llama-server executable: {:?}", output_path);
            }
        }
    }

    info!("Extracted {} files to {:?}", extracted_count, bin_dir);

    server_path.ok_or_else(|| {
        LlmError::DownloadFailed(format!(
            "{} not found in ZIP archive (extracted {} other files)",
            target_filename, extracted_count
        ))
    })
}

/// Delete the downloaded llama-server
#[allow(dead_code)]
pub async fn delete_llama_server() -> Result<()> {
    let server_path = get_llama_server_path();
    if server_path.exists() {
        tokio::fs::remove_file(&server_path).await?;
        info!("Deleted llama-server at {:?}", server_path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_models_dir() {
        let dir = get_models_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_model_path() {
        let models_dir = PathBuf::from("test_models");
        let path = get_model_path(&models_dir);
        assert!(path.ends_with(MODEL_FILENAME));
    }

    #[test]
    fn test_download_progress_percentage() {
        let progress = DownloadProgress {
            bytes_downloaded: 500,
            total_bytes: 1000,
            speed_bps: 100,
            eta_seconds: 5,
        };
        assert_eq!(progress.percentage(), 50.0);
    }

    #[test]
    fn test_needs_download_missing() {
        let temp_dir = PathBuf::from("/nonexistent/path");
        assert!(needs_download(&temp_dir));
    }

    #[test]
    fn test_model_search_dirs_includes_dot_models() {
        let dirs = model_search_dirs();
        assert!(dirs.contains(&PathBuf::from(".models")));
        assert!(dirs.contains(&PathBuf::from("models")));
        // `.models/` is scanned before the standard `models/` directory.
        let dot = dirs.iter().position(|p| p == &PathBuf::from(".models"));
        let plain = dirs.iter().position(|p| p == &PathBuf::from("models"));
        assert!(dot < plain);
    }

    #[test]
    fn test_is_size_token() {
        assert!(is_size_token("4b"));
        assert!(is_size_token("12b"));
        assert!(is_size_token("e4b"));
        assert!(is_size_token("27b"));
        assert!(!is_size_token("q4"));
        assert!(!is_size_token("b"));
        assert!(!is_size_token("mmproj"));
        assert!(!is_size_token("gemma"));
    }

    #[test]
    fn test_size_signature() {
        assert_eq!(
            size_signature("gemma-4-e4b_q4_0-it"),
            Some("e4b".to_string())
        );
        assert_eq!(
            size_signature("gemma-4-12b-it-qat-q4_0"),
            Some("12b".to_string())
        );
        assert_eq!(size_signature("qwen3.5-4b-q4_k_s"), Some("4b".to_string()));
        assert_eq!(size_signature("some-model-no-size"), None);
    }

    #[test]
    fn test_resolve_mmproj_for_pairs_by_size() {
        let dir = std::env::temp_dir().join("ss_mmproj_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let e4b = dir.join("gemma-4-E4B_q4_0-it.gguf");
        let twelveb = dir.join("gemma-4-12b-it-qat-q4_0.gguf");
        let qwen = dir.join("Qwen3.5-4B-Q4_K_S.gguf");
        let proj_e4b = dir.join("gemma-4-E4B-it-mmproj.gguf");
        let proj_12b = dir.join("mmproj-gemma-4-12b-it-qat-q4_0.gguf");
        for f in [&e4b, &twelveb, &qwen, &proj_e4b, &proj_12b] {
            std::fs::write(f, b"x").unwrap();
        }

        assert_eq!(resolve_mmproj_for(&e4b), Some(proj_e4b.clone()));
        assert_eq!(resolve_mmproj_for(&twelveb), Some(proj_12b.clone()));
        // Qwen has size sig 4b but no 4b projector → must not be paired.
        assert_eq!(resolve_mmproj_for(&qwen), None);
        // A projector is never its own projector.
        assert_eq!(resolve_mmproj_for(&proj_e4b), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_local_models_only_returns_gguf() {
        // Never panics, and only `.gguf` files are reported.
        for path in discover_local_models() {
            assert_eq!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase),
                Some("gguf".to_string())
            );
        }
    }

    #[test]
    fn test_quant_level_and_desirability() {
        assert_eq!(quant_level("gemma-4-e4b-it-qat-ud-q4_k_xl"), Some(4));
        assert_eq!(quant_level("gemma-4-e4b_q4_0-it"), Some(4));
        assert_eq!(quant_level("gemma-4-e4b-it-qat-ud-q2_k_xl"), Some(2));
        assert_eq!(quant_level("some-model-no-quant"), None);
        // Q4 is the sweet spot: preferred over a low quant and over a heavier one.
        assert!(quant_desirability("m-q4_k_xl") > quant_desirability("m-q2_k_xl"));
        assert!(quant_desirability("m-q4_k_xl") > quant_desirability("m-q8_0"));
    }

    #[test]
    fn test_pick_best_quant_prefers_q4_first_on_tie() {
        let mk = |m: &str| {
            (
                PathBuf::from(format!("{m}.gguf")),
                PathBuf::from("mmproj.gguf"),
            )
        };
        let q2 = mk("gemma-4-E4B-it-qat-UD-Q2_K_XL");
        let q4xl = mk("gemma-4-E4B-it-qat-UD-Q4_K_XL");
        let q4_0 = mk("gemma-4-E4B_q4_0-it");
        let cands = vec![q2, q4xl.clone(), q4_0];
        // Q4 beats Q2; on the Q4 tie the earliest (Q4_K_XL) wins.
        assert_eq!(pick_best_quant(cands.iter()), Some(&q4xl));
    }

    #[test]
    fn test_pref_matches_vision_stem_excludes_action_thinking() {
        // The default instruct preference resolves to the vanilla instruct build.
        assert!(pref_matches_vision_stem(
            "qwen3-vl-4b-instruct-q4_k_m",
            "qwen3-vl-4b-instruct"
        ));
        // ...but not a `*-action` fine-tune or `*-thinking` build that merely
        // shares the substring (these would otherwise tie on quant).
        assert!(!pref_matches_vision_stem(
            "sber_qwen3-vl-4b-instruct-action-q4_k_m",
            "qwen3-vl-4b-instruct"
        ));
        assert!(!pref_matches_vision_stem(
            "qwen3vl-4b-thinking-q4_k_m",
            "qwen3-vl-4b"
        ));
        // Explicitly naming the variant re-includes it (user override).
        assert!(pref_matches_vision_stem(
            "sber_qwen3-vl-4b-instruct-action-q4_k_m",
            "qwen3-vl-4b-instruct-action"
        ));
        assert!(pref_matches_vision_stem(
            "qwen3vl-4b-thinking-q4_k_m",
            "thinking"
        ));
        // Back-compat: Gemma still matches its own preference.
        assert!(pref_matches_vision_stem(
            "gemma-4-e4b-it-qat-ud-q4_k_xl",
            "gemma-4-e4b"
        ));
        // `action`/`thinking` are matched as whole tokens, not substrings: a model
        // whose name merely contains `extraction` is not excluded.
        assert!(pref_matches_vision_stem(
            "screen-extraction-qwen3-vl-4b-q4_k_m",
            "qwen3-vl-4b"
        ));
    }

    #[test]
    fn test_is_loadable_model_gguf_excludes_helpers() {
        let dir = std::env::temp_dir().join("ss_loadable_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mtp = dir.join("mtp-gemma-4-E4B-it.gguf");
        let mmproj = dir.join("gemma-4-E4B-it-mmproj.gguf");
        let partial = dir.join("gemma-4-E4B_q4_0-it.gguf");
        let embedding = dir.join("Qwen.Qwen3-VL-Embedding-2B.f16.gguf");
        for f in [&mtp, &mmproj, &partial, &embedding] {
            std::fs::write(f, b"x").unwrap();
        }
        // mtp heads and projectors are never primary models.
        assert!(!is_loadable_model_gguf(&mtp));
        assert!(!is_loadable_model_gguf(&mmproj));
        // A real model name but below the size floor (e.g. a truncated download)
        // is also rejected.
        assert!(!is_loadable_model_gguf(&partial));
        // Embedding models produce vectors, not answers — never a primary model.
        assert!(!is_loadable_model_gguf(&embedding));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pick_answer_model_prefers_vanilla_instruct() {
        // The same .models/ mix that previously mis-selected the embedding /
        // thinking models: the vanilla instruct build must win.
        let models: Vec<PathBuf> = [
            "Qwen3VL-4B-Thinking-Q4_K_M.gguf",
            "Qwen3VL-4B-Thinking-Q8_0.gguf",
            "Sber_Qwen3-VL-4B-Instruct-action-Q4_K_M.gguf",
            "qwen3-vl-4b-instruct-q4_k_m.gguf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let picked = pick_answer_model(&models).unwrap();
        assert_eq!(picked.file_name().unwrap(), "qwen3-vl-4b-instruct-q4_k_m.gguf");
        // The vanilla instruct build outscores every penalised variant.
        assert!(answer_model_score("qwen3-vl-4b-instruct-q4_k_m") > answer_model_score("qwen3vl-4b-thinking-q4_k_m"));
    }
}
