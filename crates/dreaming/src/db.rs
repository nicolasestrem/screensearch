//! Read-only DB open — mirrors `harness::export::open_readonly` and
//! `store::register_vec_extension` / `rebuild::register_vec_extension`.
//!
//! Must register `sqlite-vec` before any connection is opened. The extension is
//! required to `SELECT` from `session_vectors`/`embedding_vectors` (`vec0` tables).
//! We don't call `store` (which would open read-write) — we replicate the
//! registration via `sqlite_vec::sqlite3_vec_init` directly.

use std::path::Path;
use std::sync::Once;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};

pub fn register_vec_extension() {
    static VEC_INIT: Once = Once::new();
    VEC_INIT.call_once(|| {
        type ExtInit = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), ExtInit>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Open `path` **read-only** with `PRAGMA query_only` and a 5 s busy timeout,
/// including the WAL-recovery probe `harness::export::open_readonly` uses.
/// The caller must invoke `register_vec_extension()` before the first open in the
/// process; this helper does it idempotently.
pub fn open_readonly(path: &Path) -> Result<Connection> {
    register_vec_extension();
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {} read-only", path.display()))?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "query_only", true)?;
    // Probe — a WAL DB without a readable -shm must surface an actionable message
    // rather than a raw SQLite code (`harness::export::open_readonly` contract).
    if let Err(e) = conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        let msg = e.to_string();
        if msg.contains("readonly") || msg.contains("recovery") || msg.contains("locking") {
            bail!(
                "{path}: read-only open cannot recover the write-ahead log — the app was \
                 likely killed with an unflushed -wal. Start the app once (or close it cleanly). \
                 (underlying: {msg})",
                path = path.display()
            );
        }
        return Err(e).context("probing the database");
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_refuses_writes() {
        let dir = std::env::current_dir().unwrap().join("target").join("dreaming-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("ro_probe_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES ('hi');")
                .unwrap();
        }
        let conn = open_readonly(&path).unwrap();
        let err = conn
            .execute("INSERT INTO t VALUES ('x')", [])
            .expect_err("write should fail under query_only");
        assert!(
            err.to_string().contains("query_only")
                || err.to_string().contains("readonly")
                || err.to_string().contains("attempt to write"),
            "unexpected: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
