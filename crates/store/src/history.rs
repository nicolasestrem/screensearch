//! Reserved history identity and persisted frame allocation (`03 §7f.1/.8`).
//! Setup is the only initializer. Reads never repair missing/corrupt metadata.
//! These guarantees cover Store writers, not direct SQL, old binaries or restores.

use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use traits::{
    valid_history_id, HistoryMetadata, HistoryReadError, HistoryUnavailableReason, Result,
    FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY,
};

use crate::SqliteStore;

/// Read both keys in one SQLite statement/snapshot, including on independent
/// connections. Keeping typed readiness errors lets HTTP distinguish unavailable
/// prerequisites from ordinary storage failures without parsing error messages.
pub(crate) fn read_history_metadata(
    conn: &Connection,
) -> std::result::Result<HistoryMetadata, HistoryReadError> {
    let (identity, allocator): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT (SELECT value FROM settings WHERE key=?1),
                    (SELECT value FROM settings WHERE key=?2)",
            params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| HistoryReadError::Storage(error.into()))?;
    let history_id = identity.ok_or(HistoryReadError::Unavailable(
        HistoryUnavailableReason::MissingIdentity,
    ))?;
    if !valid_history_id(&history_id) {
        return Err(HistoryReadError::Unavailable(
            HistoryUnavailableReason::InvalidIdentity,
        ));
    }
    let allocator = allocator.ok_or(HistoryReadError::Unavailable(
        HistoryUnavailableReason::MissingAllocator,
    ))?;
    // Opaque user settings use strings, but allocator metadata must be an actual
    // nonnegative decimal i64: never coerce junk via SQLite CAST or use a default.
    let frame_id_high_water = allocator
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .filter(|_| !allocator.is_empty() && allocator.bytes().all(|b| b.is_ascii_digit()))
        .ok_or(HistoryReadError::Unavailable(
            HistoryUnavailableReason::InvalidAllocator,
        ))?;
    Ok(HistoryMetadata {
        history_id,
        frame_id_high_water,
    })
}

/// Called after migrations while opening the store, never from a read or insert.
/// Only the wholly absent pair denotes a new/pre-upgrade history. A partial or
/// invalid existing pair stays untouched and causes readiness/allocation to fail.
pub(crate) fn setup_history_metadata(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let present: i64 = tx.query_row(
        "SELECT COUNT(*) FROM settings WHERE key IN (?1,?2)",
        params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
        |row| row.get(0),
    )?;
    if present == 0 {
        // Canonical UUID v4 using SQLite's RNG, not a report identity or credential.
        // Bitmasking avoids abs(random()) overflow at the minimum signed integer.
        let history_id: String = tx.query_row(
            "SELECT lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
                    substr(hex(randomblob(2)),2) || '-' ||
                    substr('89ab',(random() & 3)+1,1) || substr(hex(randomblob(2)),2) ||
                    '-' || hex(randomblob(6)))",
            [],
            |row| row.get(0),
        )?;
        let high_water = current_max_frame_id(&tx)?;
        tx.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2),(?3,?4)",
            params![
                HISTORY_ID_KEY,
                history_id,
                FRAME_ID_HIGH_WATER_KEY,
                high_water.to_string()
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn current_max_frame_id(conn: &Connection) -> Result<i64> {
    // Existing positive IDs remain untouched. Zero is the floor for empty or
    // unusual pre-upgrade histories containing only nonpositive explicit IDs.
    let maximum: i64 =
        conn.query_row("SELECT COALESCE(MAX(id),0) FROM frames", [], |r| r.get(0))?;
    Ok(maximum.max(0))
}

/// Advance under the caller's IMMEDIATE write transaction. That lock is acquired
/// before reading either counter, so distinct SqliteStore connections serialize
/// allocation as well as insertion. Rollback also rolls back this advancement.
pub(crate) fn allocate_frame_id(tx: &Transaction<'_>) -> Result<i64> {
    let metadata = read_history_metadata(tx)?;
    let id = metadata
        .frame_id_high_water
        .max(current_max_frame_id(tx)?)
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("frame ID allocator exhausted"))?;
    tx.execute(
        "UPDATE settings SET value=?1 WHERE key=?2",
        params![id.to_string(), FRAME_ID_HIGH_WATER_KEY],
    )?;
    Ok(id)
}

impl SqliteStore {
    /// Read-only prerequisite check: no initialization, defaulting or repair.
    pub async fn history_readiness(
        &self,
    ) -> std::result::Result<HistoryMetadata, HistoryReadError> {
        self.with_conn(|conn| Ok(read_history_metadata(conn)))
            .await
            .map_err(HistoryReadError::Storage)?
    }
}
