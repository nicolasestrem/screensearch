//! Opt-in Telegram adapter. Tokens are held only in Credential Manager and call-local memory.
use std::sync::OnceLock;
use std::time::Duration;
use traits::{ReportOutcome, ReportRun, ReportRunKind};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

fn target(source_id: &str) -> Result<Vec<u16>, String> {
    uuid::Uuid::parse_str(source_id).map_err(|_| "invalid source identity".to_string())?;
    Ok(format!("ScreenSearch/Telegram/{source_id}")
        .encode_utf16()
        .chain(Some(0))
        .collect())
}
pub fn credential_read(source_id: &str) -> Result<Option<String>, String> {
    let name = target(source_id)?;
    let mut ptr = std::ptr::null_mut();
    // SAFETY: valid UTF-16 name and output pointer; CredFree releases the returned allocation.
    match unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut ptr) } {
        Ok(()) => {
            if ptr.is_null() {
                return Err("credential store returned no data".into());
            }
            // SAFETY: successful CredReadW supplies the blob and size in a live CREDENTIALW.
            let result = unsafe {
                let c = &*ptr;
                if c.CredentialBlobSize == 0 || c.CredentialBlob.is_null() {
                    Err("credential is empty".into())
                } else {
                    String::from_utf8(
                        std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
                            .to_vec(),
                    )
                    .map(Some)
                    .map_err(|_| "credential encoding is invalid".to_string())
                }
            };
            // SAFETY: pointer was allocated by Credential Manager above.
            unsafe { CredFree(ptr.cast()) };
            result
        }
        Err(e) if e.code().0 as u32 == 0x80070490 => Ok(None),
        Err(_) => Err("Windows Credential Manager is unavailable".into()),
    }
}
fn valid_token(token: &str) -> bool {
    let Some((id, key)) = token.split_once(':') else {
        return false;
    };
    !id.is_empty()
        && id.bytes().all(|c| c.is_ascii_digit())
        && key.len() >= 20
        && key.len() < 256
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
pub fn credential_write(source_id: &str, token: &str) -> Result<(), String> {
    if !valid_token(token) {
        return Err("invalid Telegram bot token format".into());
    }
    let mut name = target(source_id)?;
    let mut bytes = token.as_bytes().to_vec();
    let c = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: bytes.len() as u32,
        CredentialBlob: bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: name and credential bytes live until synchronous CredWriteW returns.
    let result =
        unsafe { CredWriteW(&c, 0) }.map_err(|_| "could not save Telegram credential".to_string());
    bytes.fill(0);
    result
}
pub fn credential_clear(source_id: &str) -> Result<(), String> {
    let name = target(source_id)?;
    // SAFETY: a live null-terminated UTF-16 credential name.
    match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(e) if e.code().0 as u32 == 0x80070490 => Ok(()),
        Err(_) => Err("could not remove Telegram credential".into()),
    }
}

fn mask(text: &str) -> String {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN.get_or_init(||regex::Regex::new(r"(?i)bearer\s+\S+|(?:api[_ -]?key|password|token|secret)\s*[:=]\s*\S+|sk-[a-z0-9_-]{8,}|\b[0-9]{6,}:[a-z0-9_-]{20,}").expect("constant credential pattern"))
        .replace_all(text,"[credential redacted]").into_owned()
}
fn bounded(text: &str, max: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max
        })
        .collect()
}
pub fn digest_chunks(run: &ReportRun) -> Result<Vec<String>, String> {
    let outcome = run
        .outcome
        .ok_or_else(|| "report has no outcome".to_string())?;
    if !matches!(outcome, ReportOutcome::Ok | ReportOutcome::Partial) {
        return Ok(vec![]);
    }
    let r = run
        .result
        .as_ref()
        .ok_or_else(|| "report has no saved output".to_string())?;
    let status = if outcome == ReportOutcome::Partial {
        "PARTIAL — incomplete coverage"
    } else {
        "OK — sampled coverage"
    };
    let body = mask(&r.markdown);
    let shortened = bounded(&body, 10_000);
    let text = format!(
        "ScreenSearch {}\n{}\n{} frames summarized; {} periods covered\n\n{}{}",
        crate::memory_files::report_label(run),
        status,
        r.frames_summarized,
        r.periods_covered,
        shortened,
        if shortened.len() < body.len() {
            "\n[Digest shortened; full report saved locally]"
        } else {
            ""
        }
    );
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut units = 0;
    for c in text.chars() {
        if units + c.len_utf16() > 3500 {
            chunks.push(std::mem::take(&mut current));
            units = 0;
        }
        current.push(c);
        units += c.len_utf16();
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    let total = chunks.len();
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| format!("[{}/{}]\n{}", i + 1, total, c))
        .collect())
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Confirmed(i64),
    Retryable { after_secs: u64 },
    Unknown,
    Permanent(String),
}

pub async fn send_chunk(token: &str, chat_id: &str, text: &str) -> SendOutcome {
    if !valid_token(token) || chat_id.trim().is_empty() || text.encode_utf16().count() > 4096 {
        return SendOutcome::Permanent("invalid credential, destination or message length".into());
    }
    let client = match reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(_) => return SendOutcome::Permanent("could not initialize Telegram transport".into()),
    };
    // Do not propagate reqwest errors: they may include the token-bearing URL.
    let response=match client.post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&serde_json::json!({"chat_id":chat_id,"text":text,"link_preview_options":{"is_disabled":true}}))
        .send().await{Ok(r)=>r,Err(_)=>return SendOutcome::Unknown};
    let status = response.status().as_u16();
    let body = match response.json::<serde_json::Value>().await {
        Ok(v) => v,
        Err(_) => return SendOutcome::Unknown,
    };
    classify_response(status, &body)
}
fn classify_response(status: u16, body: &serde_json::Value) -> SendOutcome {
    if (200..300).contains(&status) && body["ok"] == true {
        return body["result"]["message_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .map(SendOutcome::Confirmed)
            .unwrap_or(SendOutcome::Unknown);
    }
    if body["ok"] == false && body["error_code"] == 429 && status == 429 {
        return SendOutcome::Retryable {
            after_secs: body["parameters"]["retry_after"].as_u64().unwrap_or(30),
        };
    }
    if body["ok"] == false && matches!(status, 400 | 401 | 403 | 404) {
        return SendOutcome::Permanent(format!(
            "Telegram rejected the request (HTTP {status}); check credential and destination"
        ));
    }
    SendOutcome::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    fn saved_run(markdown: String) -> ReportRun {
        ReportRun {
            run_id: "report-fixture".into(),
            source_id: "source-fixture".into(),
            kind: ReportRunKind::Daily,
            schedule_id: None,
            revision_of: None,
            requested_range: None,
            window_start: 0,
            window_end: 86_400_000,
            timezone: "UTC".into(),
            local_date: "1970-01-01".into(),
            phase: traits::ReportRunPhase::Finished,
            outcome: Some(ReportOutcome::Partial),
            error: None,
            config_snapshot: "{}".into(),
            created_at: 0,
            updated_at: 0,
            result: Some(traits::SavedReport {
                markdown,
                model: Some("installed-fixture".into()),
                frame_ids: vec![7],
                session_ids: vec![],
                cited_frame_ids: vec![7],
                frames_sampled: 2,
                frames_summarized: 1,
                periods_total: 2,
                periods_covered: 1,
                passes: 1,
                truncated: true,
            }),
        }
    }
    #[test]
    fn digest_chunks_fit_limit_with_unicode_labels_and_saved_content() {
        let text = format!(
            "Saved evidence: café 東京 😀\npassword=do-not-deliver\n{}",
            "😀é東".repeat(4000)
        );
        let run = saved_run(text.clone());
        let chunks = digest_chunks(&run).unwrap();
        assert!(chunks.len() > 1);
        for (index, chunk) in chunks.iter().enumerate() {
            assert!(chunk.encode_utf16().count() <= 4096);
            assert!(chunk.starts_with(&format!("[{}/{}]\n", index + 1, chunks.len())));
            assert!(!chunk.contains('\u{fffd}'));
            assert!(!chunk.contains("do-not-deliver"));
        }
        let digest = chunks
            .iter()
            .map(|chunk| chunk.split_once('\n').unwrap().1)
            .collect::<String>();
        assert!(digest.contains("Saved evidence: café 東京 😀"));
        assert!(digest.contains("PARTIAL — incomplete coverage"));
        assert!(digest.contains("[credential redacted]"));
        assert!(digest.ends_with("[Digest shortened; full report saved locally]"));
        assert_eq!(
            run.result.as_ref().unwrap().markdown,
            text,
            "rendering must preserve the saved report"
        );
    }
    #[test]
    fn non_deliverable_outcomes_produce_no_digest() {
        for outcome in [
            ReportOutcome::NoData,
            ReportOutcome::Blocked,
            ReportOutcome::Failed,
        ] {
            let mut run = saved_run("must not be sent".into());
            run.outcome = Some(outcome);
            assert!(digest_chunks(&run).unwrap().is_empty());
        }
    }
    #[test]
    fn ambiguous_responses_never_retry() {
        assert_eq!(
            classify_response(200, &serde_json::json!({"ok":true})),
            SendOutcome::Unknown
        );
        assert_eq!(
            classify_response(502, &serde_json::json!({})),
            SendOutcome::Unknown
        );
        assert_eq!(
            classify_response(
                429,
                &serde_json::json!({"ok":false,"error_code":429,"parameters":{"retry_after":90}})
            ),
            SendOutcome::Retryable { after_secs: 90 }
        );
        assert_eq!(
            classify_response(
                200,
                &serde_json::json!({"ok":true,"result":{"message_id":123}})
            ),
            SendOutcome::Confirmed(123)
        );
    }
    #[test]
    fn supported_credentials_are_masked() {
        let text = "topic bearer abc.def token=abc password:xyz sk-123456789abcdef";
        let masked = mask(text);
        assert!(!masked.contains("abc"));
        assert!(!masked.contains("xyz"));
        assert!(!masked.contains("123456789abcdef"));
        assert!(masked.starts_with("topic"));
    }
    #[test]
    fn unicode_boundaries_preserve_scalars() {
        let text = "😀".repeat(6000);
        let short = bounded(&text, 3501);
        assert_eq!(short.encode_utf16().count(), 3500);
        assert_eq!(short.chars().count(), 1750);
    }
    #[test]
    fn digest_labels_requested_dates_without_sending() {
        let mut run = saved_run("Observed activity".into());
        run.requested_range = Some(traits::MemoryReportRange {
            start_date: "2026-10-01".into(),
            end_date: "2026-10-15".into(),
        });
        let digest = digest_chunks(&run).unwrap().join("");
        assert!(digest.contains("2026-10-01 to 2026-10-15"));
        assert!(!digest.contains("1970-01-01"));
    }
}
