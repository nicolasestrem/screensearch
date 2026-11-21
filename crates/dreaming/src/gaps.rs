//! Absence-triggered dreams — a dream per gap in `frames.captured_at`.
//!
//! Spec change from OpenClaw: dreams are triggered by **absence from the screen**
//! (a gap in activity), not by a fixed `03:00` cron. A short absence can still dream,
//! but we threshold so micro gaps (normal capture interval 2–7 s) aren't dreams.

use anyhow::Result;
use rusqlite::Connection;

/// One absence gap that triggers a dream.
#[derive(Debug, Clone)]
pub struct Gap {
    /// `frames.captured_at` of the frame before the gap (ms).
    pub before_ms: i64,
    /// `frames.captured_at` of the frame after the gap (ms).
    pub after_ms: i64,
    /// gap length in ms.
    pub gap_ms: i64,
}

/// Find all gaps `> gap_threshold_ms` in `[window_start, window_end)`.
/// Returns sorted by `after_ms`. An empty `frames` table yields no gaps.
pub fn find_gaps(conn: &Connection, window_start: i64, window_end: i64, gap_threshold_ms: i64) -> Result<Vec<Gap>> {
    let mut stmt = conn.prepare(
        "SELECT captured_at FROM frames
         WHERE captured_at >= ?1 AND captured_at < ?2
         ORDER BY captured_at ASC",
    )?;
    let times: Vec<i64> = stmt
        .query_map([window_start, window_end], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if times.len() < 2 {
        return Ok(Vec::new());
    }
    let mut gaps = Vec::new();
    for w in times.windows(2) {
        let delta = w[1] - w[0];
        if delta > gap_threshold_ms {
            gaps.push(Gap {
                before_ms: w[0],
                after_ms: w[1],
                gap_ms: delta,
            });
        }
    }
    Ok(gaps)
}

/// Derive dream windows from gaps: "since the last dream" = `(prev_gap.after_ms, gap.before_ms]`
/// (or `[window_start, gap.before_ms]` for the first dream). The final flush is optional
/// and not a dream trigger — only gaps trigger dreams per spec.
pub fn dream_windows_from_gaps(gaps: &[Gap], window_start: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut cursor = window_start;
    for gap in gaps {
        if gap.before_ms > cursor {
            out.push((cursor, gap.before_ms));
        }
        cursor = gap.after_ms;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn seed(times: &[i64]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE frames(id INTEGER PRIMARY KEY, captured_at INTEGER NOT NULL)").unwrap();
        for &t in times {
            conn.execute("INSERT INTO frames(captured_at) VALUES (?1)", [t]).unwrap();
        }
        conn
    }

    #[test]
    fn finds_only_large_gaps() {
        let conn = seed(&[0, 1_000, 2_000, 600_000, 601_000, 1_500_000]);
        let gaps = find_gaps(&conn, 0, 2_000_000, 10_000).unwrap();
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].gap_ms, 598_000);
        assert_eq!(gaps[1].gap_ms, 899_000);
    }

    #[test]
    fn windows_cover_since_last_dream() {
        let gaps = vec![
            Gap { before_ms: 10_000, after_ms: 50_000, gap_ms: 40_000 },
            Gap { before_ms: 100_000, after_ms: 200_000, gap_ms: 100_000 },
        ];
        let w = dream_windows_from_gaps(&gaps, 0);
        assert_eq!(w, vec![(0, 10_000), (50_000, 100_000)]);
    }
}
