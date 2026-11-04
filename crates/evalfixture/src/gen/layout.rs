//! Screen geometry: turns a rendered screen into OCR word spans the way WinRT reports them.
//!
//! The production attention filter (`crates/textfilter`) decides what survives into
//! `content_text` from span geometry alone, so the fixture places every line where the real
//! thing would be and lets the filter do its job:
//!
//! - **body** lines sit in the interior of the foreground window: always kept as content;
//! - **top / side / bottom** lines (menus, tabs, sidebars, status bars) sit on the window
//!   edges: catalogued as static chrome and suppressed once the same label has been seen
//!   `text.chrome_suppress_min_seen` times;
//! - the **title bar** echoes the window title: always dropped as chrome;
//! - **background** lines belong to another window to the right of a windowed app: dropped;
//! - the **taskbar** (search box, clock, input language) sits below the window in the
//!   bottom band: dropped as system UI.
//!
//! Lines are emitted in reading order blocks (top, side, body, bottom, background, taskbar),
//! each OCR line with its own `line_index`, and `raw_text` joins them with `\n` exactly like
//! the WinRT provider (`crates/ocr`).

use anyhow::{ensure, Result};
use traits::{normalize_text, OcrResult, TextFilterContext, TextRole, TextSource, TextSpan};

/// Top of the Windows taskbar on a 1080-line display (1032 / 1080).
const TASKBAR_TOP: f32 = 0.9556;
/// Glyph advance as a fraction of the window width (about 9.6 px at 1920 px).
const CHAR_W: f32 = 0.0050;
/// Sidebar labels longer than this would reach the window interior; they are truncated.
pub(crate) const SIDE_MAX_CHARS: usize = 20;
/// Background-window lines are truncated to the visible part of that window.
const BACKGROUND_MAX_CHARS: usize = 52;
/// At most this many lines fit in the top edge band (title bar included).
const TOP_MAX_LINES: usize = 5;

/// `CONFIDENCE_UNKNOWN` of the WinRT provider: the engine reports no per-word confidence.
const CONFIDENCE_UNKNOWN: f32 = -1.0;

/// One captured screen before layout.
#[derive(Debug, Clone, Default)]
pub(crate) struct Screen {
    pub app_hint: &'static str,
    pub title: String,
    pub url: Option<String>,
    /// A windowed app leaves room for another window on the right (background text).
    pub windowed: bool,
    /// Whether the app draws its window title in a title bar (echoed text, always chrome).
    pub title_bar: bool,
    pub top: Vec<String>,
    pub side: Vec<String>,
    pub body: Vec<String>,
    pub bottom: Vec<String>,
    pub background: Vec<String>,
}

/// The OCR result and filter context for one laid-out screen.
pub(crate) struct Laid {
    pub ocr: OcrResult,
    pub ctx: TextFilterContext,
}

struct Line {
    text: String,
    x: f32,
    y: f32,
    h: f32,
    char_w: f32,
}

fn window_rect(windowed: bool) -> [f32; 4] {
    if windowed {
        [0.06, 0.05, 0.62, 0.86]
    } else {
        [0.0, 0.0, 1.0, TASKBAR_TOP]
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Lays out `screen` on a 1920x1080 display with the taskbar showing `clock` (`HH:MM` and
/// the date). Fails if a body line would be dropped as a title echo, which would silently
/// remove content the generator meant to keep.
pub(crate) fn lay_out(screen: &Screen, clock: (&str, &str)) -> Result<Laid> {
    let title_norm = normalize_text(&screen.title);
    for line in &screen.body {
        ensure!(
            title_norm.is_empty() || normalize_text(line) != title_norm,
            "body line repeats the window title and would be filtered: {line}"
        );
    }

    let [rx, ry, rw, rh] = window_rect(screen.windowed);
    let mut lines: Vec<Line> = Vec::new();

    // Top edge band: optional title bar, then menus/tabs/omnibox.
    let mut top: Vec<(String, bool)> = Vec::new();
    if screen.title_bar && !screen.title.is_empty() {
        top.push((screen.title.clone(), true));
    }
    top.extend(screen.top.iter().map(|t| (t.clone(), false)));
    ensure!(
        top.len() <= TOP_MAX_LINES,
        "{} top lines exceed the {TOP_MAX_LINES}-line edge band",
        top.len()
    );
    for (k, (text, centered)) in top.into_iter().enumerate() {
        let len = text.chars().count().max(1) as f32;
        let char_w = (CHAR_W * rw).min(0.97 * rw / len);
        let width = len * char_w;
        let x = if centered {
            rx + ((rw - width) / 2.0).max(0.01 * rw)
        } else {
            rx + 0.012 * rw
        };
        lines.push(Line {
            text,
            x,
            y: ry + (0.004 + 0.0115 * k as f32) * rh,
            h: 0.0095 * rh,
            char_w,
        });
    }

    // Left sidebar: short labels hugging the left edge.
    for (i, text) in screen.side.iter().take(24).enumerate() {
        lines.push(Line {
            text: truncate_chars(text, SIDE_MAX_CHARS),
            x: rx + 0.004 * rw,
            y: ry + (0.075 + 0.032 * i as f32) * rh,
            h: 0.014 * rh,
            char_w: CHAR_W * rw,
        });
    }

    // Body: the window interior.
    let n = screen.body.len().max(1) as f32;
    let step = (0.84 / n).min(0.028);
    let x0 = rx
        + if screen.side.is_empty() {
            0.07 * rw
        } else {
            0.13 * rw
        };
    let available = rx + 0.985 * rw - x0;
    for (i, text) in screen.body.iter().enumerate() {
        let len = text.chars().count().max(1) as f32;
        lines.push(Line {
            text: text.clone(),
            x: x0,
            y: ry + (0.075 + step * i as f32) * rh,
            h: (0.6 * step).max(0.006) * rh,
            char_w: (CHAR_W * rw).min(available / len),
        });
    }

    // Bottom edge: status bar parts spread along one row.
    let slots = screen.bottom.len().max(1) as f32;
    let slot_w = 0.98 * rw / slots;
    for (j, text) in screen.bottom.iter().enumerate() {
        let len = text.chars().count().max(1) as f32;
        lines.push(Line {
            text: text.clone(),
            x: rx + 0.01 * rw + slot_w * j as f32,
            y: ry + 0.976 * rh,
            h: 0.0115 * rh,
            char_w: (CHAR_W * rw).min(0.95 * slot_w / len),
        });
    }

    // Another window to the right of a windowed app.
    if screen.windowed {
        for (i, text) in screen.background.iter().take(24).enumerate() {
            lines.push(Line {
                text: truncate_chars(text, BACKGROUND_MAX_CHARS),
                x: 0.71,
                y: 0.08 + 0.03 * i as f32,
                h: 0.016,
                char_w: CHAR_W,
            });
        }
    }

    // Taskbar: search box, input language and the clock, below every window.
    for (text, x, y) in [
        ("Search", 0.06, 0.968),
        ("ENG", 0.915, 0.968),
        (clock.0, 0.946, 0.961),
        (clock.1, 0.938, 0.979),
    ] {
        lines.push(Line {
            text: text.to_string(),
            x,
            y,
            h: 0.014,
            char_w: 0.0042,
        });
    }

    let mut raw: Vec<String> = Vec::with_capacity(lines.len());
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut line_index: u32 = 0;
    for line in &lines {
        let words: Vec<&str> = line.text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let mut cursor = line.x;
        for word in &words {
            let w = word.chars().count() as f32 * line.char_w;
            spans.push(TextSpan {
                text: (*word).to_string(),
                normalized_text: normalize_text(word),
                source: TextSource::Ocr,
                role: TextRole::Unknown,
                x: cursor,
                y: line.y,
                w,
                h: line.h,
                line_index,
                is_searchable: true,
                suppress_reason: None,
            });
            cursor += w + line.char_w;
        }
        raw.push(words.join(" "));
        line_index += 1;
    }

    let text = traits::TextAdvanced::default();
    Ok(Laid {
        ocr: OcrResult {
            text: raw.join("\n").trim_end().to_string(),
            mean_confidence: CONFIDENCE_UNKNOWN,
            engine: "evalfixture".to_string(),
            spans,
        },
        ctx: TextFilterContext {
            target_rect: Some([rx, ry, rw, rh]),
            chrome_suppress_min_seen: text.chrome_suppress_min_seen,
            chrome_protect_min_chars: text.chrome_protect_min_chars,
            chrome_region_buckets: text.chrome_region_buckets,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Screen {
        Screen {
            app_hint: "Code",
            title: r"C:\dev\x\src\a.rs - x - Visual Studio Code".to_string(),
            title_bar: true,
            top: vec!["File Edit Selection View Go Run Terminal Help".to_string()],
            side: vec![
                "EXPLORER".to_string(),
                "a-very-long-sidebar-label-indeed".to_string(),
            ],
            body: vec![
                "1 use std::time::Duration;".to_string(),
                r"PS C:\dev\x> cargo test".to_string(),
            ],
            bottom: vec!["main".to_string(), "Ln 4, Col 2".to_string()],
            ..Screen::default()
        }
    }

    #[test]
    fn spans_follow_reading_blocks_and_raw_text_joins_lines() {
        let laid = lay_out(&screen(), ("09:41", "07/09/2026")).unwrap();
        let first_line: Vec<&str> = laid
            .ocr
            .spans
            .iter()
            .filter(|s| s.line_index == 0)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(
            first_line.join(" "),
            r"C:\dev\x\src\a.rs - x - Visual Studio Code"
        );
        assert!(laid.ocr.text.contains("PS C:\\dev\\x> cargo test"));
        assert!(laid.ocr.text.ends_with("07/09/2026"));
        // The sidebar label was truncated so its centroid stays on the edge.
        assert!(laid.ocr.text.contains("a-very-long-sidebar"));
        assert!(!laid.ocr.text.contains("a-very-long-sidebar-label"));
        let rect = laid.ctx.target_rect.unwrap();
        for span in &laid.ocr.spans {
            assert!(span.x >= 0.0 && span.x + span.w <= 1.0, "{span:?}");
            assert!(span.y >= 0.0 && span.y + span.h <= 1.0, "{span:?}");
        }
        assert_eq!(rect, [0.0, 0.0, 1.0, TASKBAR_TOP]);
    }

    #[test]
    fn a_body_line_equal_to_the_title_is_refused() {
        let mut s = screen();
        s.body.push(s.title.clone());
        assert!(lay_out(&s, ("09:41", "07/09/2026")).is_err());
    }
}
