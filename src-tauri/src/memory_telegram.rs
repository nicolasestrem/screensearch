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
/// The frozen daily digest (`03 §8d`). Any edit here changes what users already receive;
/// weekly rendering lives in [`weekly_digest`] and must not leak into this path.
fn daily_digest(run: &ReportRun, r: &traits::SavedReport, outcome: ReportOutcome) -> String {
    let status = if outcome == ReportOutcome::Partial {
        "PARTIAL — incomplete coverage"
    } else {
        "OK — sampled coverage"
    };
    let body = mask(&r.markdown);
    let shortened = bounded(&body, 10_000);
    format!(
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
    )
}

/// Local `YYYY-MM-DD` days the saved weekly report itself discloses as lacking a complete
/// saved daily (`03 §8e`). `save_weekly_execution` prepends that banner to the saved
/// markdown of a partial week only, so the digest never has to infer missing days from run
/// metadata it does not carry. Only strict dates are echoed; anything else in the sentence
/// is ignored. On a non-partial week a leading banner literal can only be model prose, so
/// nothing is parsed there and model-implied date markers never leak into the digest.
fn weekly_missing_daily_days(markdown: &str, outcome: ReportOutcome) -> Vec<String> {
    if outcome != ReportOutcome::Partial {
        return Vec::new();
    }
    let banner = markdown
        .split_once("\n\n")
        .map_or(markdown, |(head, _)| head);
    if !banner.starts_with("**Partial coverage:** ") {
        return Vec::new();
    }
    const MARKER: &str = "the week is missing complete saved daily coverage for ";
    let Some((_, rest)) = banner.split_once(MARKER) else {
        return Vec::new();
    };
    let list = rest.split(['.', ';', '\n']).next().unwrap_or_default();
    let mut days: Vec<String> = Vec::new();
    for candidate in list.split(',') {
        let day = candidate.trim();
        if kernel::memory_schedule::parse_date(day).is_ok() && !days.iter().any(|seen| seen == day)
        {
            days.push(day.to_owned());
        }
    }
    days
}

/// The weekly Telegram digest (`03 §8e` / G6): the ISO week label, the saved run's
/// truthful status and measured coverage, the missing-daily disclosure the saved report
/// carries, the saved daily-run source IDs, then the saved synthesis (masked, bounded).
/// Pure rendering: no model call, no store read, no re-inference.
fn weekly_digest(
    run: &ReportRun,
    r: &traits::SavedReport,
    outcome: ReportOutcome,
) -> Result<String, String> {
    let label = kernel::week_label(&run.timezone, run.window_start)
        .map_err(|_| "weekly report has no valid week label".to_string())?;
    let missing = weekly_missing_daily_days(&r.markdown, outcome);
    let complete_days = r.periods_total == 7 && r.periods_covered == 7;
    let status = if outcome == ReportOutcome::Partial {
        if !complete_days || !missing.is_empty() {
            "PARTIAL — incomplete coverage"
        } else {
            "PARTIAL — saved synthesis has limitations"
        }
    } else if complete_days {
        "OK — complete saved-daily coverage"
    } else {
        "OK — saved synthesis"
    };
    // A day whose only saved daily is partial has evidence (`periods_covered`) but is
    // still named below; never count it as covered next to its own name.
    let covered = if missing.is_empty() {
        r.periods_covered
    } else {
        r.periods_covered
            .min(r.periods_total.saturating_sub(missing.len() as u32))
    };
    let mut coverage = format!("Coverage: {}/{} days covered", covered, r.periods_total);
    if !missing.is_empty() {
        coverage.push_str(&format!(
            "; missing or partial saved dailies: {}",
            missing.join(", ")
        ));
    } else if r.periods_covered < r.periods_total {
        coverage.push_str("; missing or partial saved dailies: not enumerated in saved report");
    }
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<_> = r
        .daily_run_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .collect();
    let sources = if ids.is_empty() {
        "Sources: no saved daily reports recorded".to_string()
    } else {
        format!(
            "Sources: {} saved daily {} — {}",
            ids.len(),
            if ids.len() == 1 { "report" } else { "reports" },
            {
                let mut text = ids
                    .iter()
                    .take(12)
                    .map(|id| {
                        let masked = mask(id);
                        let prefix = bounded(&masked, 96);
                        if prefix.len() < masked.len() {
                            format!("{prefix}…")
                        } else {
                            prefix
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                if ids.len() > 12 {
                    text.push_str(&format!(
                        "; +{} more (full source list in saved report)",
                        ids.len() - 12
                    ));
                }
                text
            }
        )
    };
    let body = mask(&r.markdown);
    let shortened = bounded(&body, 10_000);
    let cut = shortened.encode_utf16().count() < body.encode_utf16().count();
    Ok(format!(
        "ScreenSearch {label}\n{status}\n{coverage}\n{sources}\n{} frames summarized\n\nHighlights\n{}{}",
        r.frames_summarized,
        shortened,
        if cut {
            "\n[Digest shortened; full report saved locally]"
        } else {
            ""
        }
    ))
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
    let text = match run.kind {
        ReportRunKind::Daily => daily_digest(run, r, outcome),
        ReportRunKind::WeeklySynthesis => weekly_digest(run, r, outcome)?,
    };
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
            kind: traits::ReportRunKind::Daily,
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
                daily_run_ids: Vec::new(),
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

    /// A saved weekly synthesis (`03 §8e`) over Europe/Paris ISO week 2026-W42.
    fn weekly_saved_run(markdown: String) -> ReportRun {
        let mut run = saved_run(markdown);
        run.kind = traits::ReportRunKind::WeeklySynthesis;
        run.window_start = chrono::DateTime::parse_from_rfc3339("2026-10-11T22:00:00Z")
            .unwrap()
            .timestamp_millis();
        run.window_end = chrono::DateTime::parse_from_rfc3339("2026-10-18T22:00:00Z")
            .unwrap()
            .timestamp_millis();
        run.timezone = "Europe/Paris".into();
        run.local_date = "2026-10-12".into();
        run.outcome = Some(ReportOutcome::Partial);
        let result = run.result.as_mut().unwrap();
        result.periods_total = 7;
        result.periods_covered = 5;
        result.frames_summarized = 12;
        result.daily_run_ids = vec![
            "11111111-1111-4111-8111-111111111111".into(),
            "22222222-2222-4222-8222-222222222222".into(),
        ];
        run
    }

    #[test]
    fn weekly_digest_uses_saved_weekly_report() {
        let markdown = "**Partial coverage:** the week is missing complete saved daily coverage for 2026-10-15.\n\n## Saved synthesis\n\nShipped the weekly digest [frame 101]; reviewed 東京 😀 notes.";
        let run = weekly_saved_run(markdown.into());
        let digest = digest_chunks(&run).unwrap().join("");

        // The digest is rendered from the saved synthesis, never re-inferred.
        assert!(digest.contains("Saved synthesis"));
        assert!(digest.contains("Shipped the weekly digest [frame 101]"));
        assert!(digest.contains("reviewed 東京 😀 notes"));
        // Provenance comes from the saved report's own daily-run IDs.
        assert!(digest.contains("11111111-1111-4111-8111-111111111111"));
        assert!(digest.contains("22222222-2222-4222-8222-222222222222"));
        assert_eq!(
            run.result.as_ref().unwrap().markdown,
            markdown,
            "rendering must preserve the saved weekly report"
        );
    }

    #[test]
    fn weekly_digest_renders_weekly_format() {
        let markdown = "**Partial coverage:** the week is missing complete saved daily coverage for 2026-10-15.\n\n## Activity\n\nShipped the weekly digest [frame 101].";
        let run = weekly_saved_run(markdown.into());
        let chunks = digest_chunks(&run).unwrap();
        let digest = chunks
            .iter()
            .map(|chunk| chunk.split_once('\n').unwrap().1)
            .collect::<String>();

        assert!(
            digest.starts_with(
                "ScreenSearch 2026-W42\nPARTIAL — incomplete coverage\nCoverage: 5/7 days covered; missing or partial saved dailies: 2026-10-15\n"
            ),
            "weekly header must carry the ISO week, truthful status, coverage and missing day: {digest}"
        );
        assert!(digest.contains(
            "Sources: 2 saved daily reports — 11111111-1111-4111-8111-111111111111, 22222222-2222-4222-8222-222222222222"
        ));
        assert!(digest.contains("12 frames summarized\n\nHighlights\n"));
        assert!(digest.contains("Shipped the weekly digest [frame 101]"));
        // A weekly run never borrows the daily local-date label.
        assert!(!digest.starts_with("ScreenSearch 2026-10-12"));

        // Complete measured coverage is stated as such, with no invented gaps.
        let mut complete = weekly_saved_run("## Activity\n\nAll seven days saved.".into());
        complete.outcome = Some(ReportOutcome::Ok);
        complete.result.as_mut().unwrap().periods_covered = 7;
        let complete_digest = digest_chunks(&complete)
            .unwrap()
            .iter()
            .map(|chunk| chunk.split_once('\n').unwrap().1)
            .collect::<String>();
        assert!(complete_digest.starts_with(
            "ScreenSearch 2026-W42\nOK — complete saved-daily coverage\nCoverage: 7/7 days covered\n"
        ));
        assert!(!complete_digest.contains("missing or partial saved dailies"));
    }

    #[test]
    fn weekly_digest_never_counts_a_named_partial_day_as_covered() {
        // Seven days carry evidence, but 2026-10-15's only saved daily was partial.
        let markdown = "**Partial coverage:** the week is missing complete saved daily coverage for 2026-10-15.\n\n## Activity\n\nShipped [frame 101].";
        let mut run = weekly_saved_run(markdown.into());
        run.result.as_mut().unwrap().periods_covered = 7;
        let digest = digest_chunks(&run)
            .unwrap()
            .iter()
            .map(|chunk| chunk.split_once('\n').unwrap().1)
            .collect::<String>();
        assert!(
            digest.contains(
                "Coverage: 6/7 days covered; missing or partial saved dailies: 2026-10-15\n"
            ),
            "a named incomplete day is not counted as covered: {digest}"
        );
    }

    #[test]
    fn weekly_digest_chunks_fit_telegram_limit() {
        let text = format!(
            "Week notes: café 東京 😀\npassword=weekly-secret\n{}",
            "😀é東".repeat(4000)
        );
        let run = weekly_saved_run(text.clone());
        let chunks = digest_chunks(&run).unwrap();
        assert!(chunks.len() > 1);
        for (index, chunk) in chunks.iter().enumerate() {
            assert!(chunk.encode_utf16().count() <= 4096);
            assert!(chunk.starts_with(&format!("[{}/{}]\n", index + 1, chunks.len())));
            assert!(!chunk.contains('\u{fffd}'));
            assert!(!chunk.contains("weekly-secret"));
        }
        let digest = chunks
            .iter()
            .map(|chunk| chunk.split_once('\n').unwrap().1)
            .collect::<String>();
        assert!(digest.contains("ScreenSearch 2026-W42"));
        assert!(digest.contains("Week notes: café 東京 😀"));
        assert!(digest.contains("[credential redacted]"));
        assert!(digest.ends_with("[Digest shortened; full report saved locally]"));
        assert_eq!(
            run.result.as_ref().unwrap().markdown,
            text,
            "rendering must preserve the saved weekly report"
        );
    }

    #[test]
    fn weekly_digest_partial_truncation_does_not_claim_missing_days() {
        let mut run = weekly_saved_run("Saved synthesis has bounded sampling.".into());
        let report = run.result.as_mut().unwrap();
        report.periods_covered = 7;
        report.truncated = true;
        let digest = digest_chunks(&run).unwrap().join("");
        assert!(digest.contains("PARTIAL — saved synthesis has limitations"));
        assert!(digest.contains("Coverage: 7/7 days covered"));
        assert!(!digest.contains("missing or partial saved dailies"));
    }

    #[test]
    fn weekly_digest_ignores_model_echoed_missing_day_markers() {
        let run = weekly_saved_run("**Partial coverage:** sampling was bounded.\n\nModel echo: the week is missing complete saved daily coverage for 2099-01-01.".into());
        let digest = digest_chunks(&run).unwrap().join("");
        let coverage = digest
            .lines()
            .find(|line| line.starts_with("Coverage:"))
            .unwrap();
        assert!(coverage.contains("not enumerated in saved report"));
        assert!(!coverage.contains("2099"));
    }

    #[test]
    fn weekly_digest_bounds_and_deduplicates_source_ids() {
        let mut run = weekly_saved_run("Saved synthesis stays intact.".into());
        let ids: Vec<String> = (0..100).map(|i| format!("daily-run-{i:03}")).collect();
        let report = run.result.as_mut().unwrap();
        report.daily_run_ids = ids.clone();
        report.daily_run_ids.push(ids[0].clone());
        let digest = digest_chunks(&run).unwrap().join("");
        assert!(digest.contains("Sources: 100 saved daily reports"));
        assert_eq!(digest.matches(&ids[0]).count(), 1);
        assert!(digest.contains(&ids[11]));
        assert!(!digest.contains(&ids[12]));
        assert!(digest.contains("+88 more"));
        assert!(digest.contains("Saved synthesis stays intact."));
    }

    #[test]
    fn weekly_digest_abbreviates_unusually_long_source_ids() {
        let mut run = weekly_saved_run("Saved synthesis stays intact.".into());
        let long_id = format!("daily-run-{}", "x".repeat(110));
        let report = run.result.as_mut().unwrap();
        report.daily_run_ids = vec![long_id.clone()];
        let digest = digest_chunks(&run).unwrap().join("");
        assert!(
            digest.contains(&format!("{}…", &long_id[..96])),
            "an unusually long id is visibly abbreviated"
        );
        assert!(!digest.contains(&long_id));
    }

    #[test]
    fn weekly_digest_ignores_leading_banner_on_ok_week() {
        let mut run = weekly_saved_run(
            "**Partial coverage:** the week is missing complete saved daily coverage for \
             2099-01-01.\n\n## Activity\n\nModel prose opening the report."
                .into(),
        );
        run.outcome = Some(ReportOutcome::Ok);
        let digest = digest_chunks(&run).unwrap().join("");
        // The saved synthesis is reproduced verbatim under Highlights; the invariant is
        // that the disclosure line never turns model prose into claimed missing days.
        let coverage = digest
            .lines()
            .find(|line| line.starts_with("Coverage:"))
            .unwrap();
        assert!(
            !coverage.contains("2099"),
            "an OK week must not echo model-implied missing-day markers: {coverage}"
        );
        assert!(coverage.contains("not enumerated in saved report"));
        assert!(digest.contains("OK — saved synthesis"));
    }

    #[test]
    fn daily_digest_output_is_unchanged() {
        let run = saved_run("Observed activity".into());
        assert_eq!(
            digest_chunks(&run).unwrap(),
            vec![
                "[1/1]\nScreenSearch 1970-01-01\nPARTIAL — incomplete coverage\n1 frames summarized; 1 periods covered\n\nObserved activity"
                    .to_string()
            ],
            "the daily digest format is frozen"
        );
    }
}
