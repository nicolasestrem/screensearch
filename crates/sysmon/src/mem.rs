//! Host memory pressure: whole-machine physical usage from `GlobalMemoryStatusEx` plus
//! whole-machine commit charge from `GetPerformanceInfo` (`03 §5/§8`).
//!
//! This is the throttle's third arm. CPU and GPU pressure only see a *busy* machine; the
//! case this arm exists for is a machine that is out of memory while ScreenSearch itself
//! is doing nothing — a large editor, another recorder, several browsers. Both readings are
//! turned into **usage** percentages so they share the CPU/GPU hysteresis convention
//! (`*_exit_pct` strictly below `*_enter_pct`); the two byte counts are kept as well so the
//! UI can show the honest "12 331 MiB available" rather than only a percentage.
//!
//! **Why commit charge does not come from `MEMORYSTATUSEX`.** Its physical fields are
//! whole-machine, but its page-file fields are process-scoped: Microsoft documents
//! `ullTotalPageFile` as "the current committed memory limit for the system or the current
//! process, whichever is smaller" and `ullAvailPageFile` as "the maximum amount of memory
//! the current process can commit", which "is equal to or smaller than the system-wide
//! available commit value" — and names `GetPerformanceInfo` for the system-wide numbers.
//! Both fields therefore describe *this process's* headroom, so their difference is neither
//! the host's charge nor a stable fraction of it. The host numbers come from
//! `PERFORMANCE_INFORMATION`: `CommitTotal` and `CommitLimit` are page counts, scaled by
//! `PageSize`.
//!
//! Both syscalls must succeed. The sample carries a single `mem_monitored` flag for the arm,
//! so a partial reading is reported as unmonitored rather than as a fabricated `0 %` commit
//! that would silently disarm the commit threshold. There is no handle to open and no latch
//! to keep: two calls per sample, and a failure degrades to `None` for that sample only.

/// One memory reading. Percentages are `0..=100`; byte counts are MiB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MemoryStatus {
    /// Physical-memory usage: `1 - available/total`.
    pub used_pct: f32,
    /// Commit-charge usage: `charge / limit`.
    pub commit_pct: f32,
    pub avail_phys_mb: u64,
    pub commit_charge_mb: u64,
    pub commit_limit_mb: u64,
}

/// Raw platform counters: physical memory already in bytes (`GlobalMemoryStatusEx`), commit
/// as page counts plus the host page size (`PERFORMANCE_INFORMATION` is page-based).
struct PlatformMemory {
    total_phys: u64,
    avail_phys: u64,
    commit_total_pages: usize,
    commit_limit_pages: usize,
    page_size: usize,
}

/// `used` as a percentage of `total`, clamped to the `0..=100` contract. A zero total (no
/// basis for a ratio) yields `0.0` instead of a NaN, and an over-large `used` clamps instead
/// of exceeding the contract.
fn pct(used_bytes: u64, total_bytes: u64) -> f32 {
    if total_bytes == 0 {
        return 0.0;
    }
    let share = (used_bytes as f64 / total_bytes as f64) * 100.0;
    (share as f32).clamp(0.0, 100.0)
}

/// Bytes from a page count, saturating rather than wrapping on a nonsensical pair.
fn pages_to_bytes(pages: usize, page_size: usize) -> u64 {
    (pages as u64).saturating_mul(page_size as u64)
}

/// Whole MiB, rounded down. Rounding "available" up would overstate what the machine can
/// still hand out, and dividing cannot overflow.
fn mib(bytes: u64) -> u64 {
    bytes / (1 << 20)
}

/// Turns raw counters into a reading, or `None` when a counter cannot support a truthful
/// percentage (no physical total, or a zero commit limit/page size).
fn status_from(raw: PlatformMemory) -> Option<MemoryStatus> {
    let commit_charge = pages_to_bytes(raw.commit_total_pages, raw.page_size);
    let commit_limit = pages_to_bytes(raw.commit_limit_pages, raw.page_size);
    if raw.total_phys == 0 || commit_limit == 0 {
        return None;
    }
    let used_phys = raw.total_phys.saturating_sub(raw.avail_phys);
    Some(MemoryStatus {
        used_pct: pct(used_phys, raw.total_phys),
        commit_pct: pct(commit_charge, commit_limit),
        avail_phys_mb: mib(raw.avail_phys),
        commit_charge_mb: mib(commit_charge),
        commit_limit_mb: mib(commit_limit),
    })
}

/// Reads the host's current memory status, or `None` when a platform call fails or returns
/// counters that cannot form a truthful percentage (the probe then reports
/// `mem_monitored = false` for that sample — never a fabricated `0 %`).
pub(crate) fn read() -> Option<MemoryStatus> {
    status_from(read_platform()?)
}

#[cfg(windows)]
fn read_platform() -> Option<PlatformMemory> {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` is a valid out-param whose `dwLength` is the struct size, exactly as
    // the API requires; the pointer is valid for the duration of the call and nothing else
    // observes it.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;

    let info_cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let mut info = PERFORMANCE_INFORMATION {
        cb: info_cb,
        ..Default::default()
    };
    // SAFETY: `info` is a valid out-param and `info_cb` is its size in bytes, exactly as the
    // API requires; the pointer is valid for the duration of the call and nothing else
    // observes it.
    unsafe { GetPerformanceInfo(&mut info, info_cb) }.ok()?;

    Some(PlatformMemory {
        total_phys: status.ullTotalPhys,
        avail_phys: status.ullAvailPhys,
        commit_total_pages: info.CommitTotal,
        commit_limit_pages: info.CommitLimit,
        page_size: info.PageSize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn raw(
        total_phys: u64,
        avail_phys: u64,
        commit_total_pages: usize,
        commit_limit_pages: usize,
        page_size: usize,
    ) -> PlatformMemory {
        PlatformMemory {
            total_phys,
            avail_phys,
            commit_total_pages,
            commit_limit_pages,
            page_size,
        }
    }

    #[test]
    fn pct_is_the_used_share_and_stays_in_range() {
        assert_eq!(pct(24 * GIB, 32 * GIB), 75.0);
        assert_eq!(pct(0, 32 * GIB), 0.0);
        assert_eq!(pct(32 * GIB, 32 * GIB), 100.0);
        assert!(pct(1, 32 * GIB) > 0.0);
        // A counter pair that disagrees with itself cannot escape 0..=100.
        assert_eq!(pct(64 * GIB, 32 * GIB), 100.0);
        // No total, no percentage — and never a NaN.
        assert_eq!(pct(0, 0), 0.0);
        assert_eq!(pct(5, 0), 0.0);
    }

    #[test]
    fn mib_rounds_down() {
        assert_eq!(mib(0), 0);
        assert_eq!(mib((1 << 20) - 1), 0);
        assert_eq!(mib(1 << 20), 1);
        assert_eq!(mib(u64::MAX), u64::MAX >> 20);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn pages_to_bytes_saturates_instead_of_wrapping() {
        assert_eq!(pages_to_bytes(usize::MAX, 2), u64::MAX);
    }

    #[test]
    fn status_reports_the_system_wide_commit_charge() {
        // 4 KiB pages: 8 000 000 charged of a 10 000 000-page system limit (RAM + page
        // file). The percentage is the host's charge/limit, not a process allowance.
        let m = status_from(raw(32 * GIB, 8 * GIB, 8_000_000, 10_000_000, 4096))
            .expect("plausible counters");
        assert_eq!(m.used_pct, 75.0);
        assert!((m.commit_pct - 80.0).abs() < 0.001, "commit {m:?}");
        assert_eq!(m.avail_phys_mb, 8192);
        assert_eq!(m.commit_charge_mb, mib(8_000_000u64 * 4096));
        assert_eq!(m.commit_limit_mb, mib(10_000_000u64 * 4096));
        assert!(m.commit_charge_mb < m.commit_limit_mb, "charge {m:?}");
    }

    #[test]
    fn status_rejects_counters_that_cannot_form_a_percentage() {
        // No physical total, a zero commit limit, and a zero page size (which collapses the
        // page counts) are all reported as unmonitored rather than as a fabricated 0 %.
        assert!(status_from(raw(0, 0, 1, 1, 4096)).is_none());
        assert!(status_from(raw(32 * GIB, 8 * GIB, 1, 0, 4096)).is_none());
        assert!(status_from(raw(32 * GIB, 8 * GIB, 1, 1, 0)).is_none());
    }

    #[test]
    fn status_clamps_inconsistent_counter_snapshots() {
        // The two syscalls are separate snapshots: a pagefile shrink or drain can land
        // between them so the charge outruns the limit, and available physical can exceed
        // the total on some hypervisors. Neither may escape 0..=100.
        let m = status_from(raw(16 * GIB, 32 * GIB, 11, 10, 1 << 30)).expect("counters");
        assert_eq!(m.used_pct, 0.0);
        assert_eq!(m.commit_pct, 100.0);
    }

    #[cfg(windows)]
    #[test]
    fn host_reports_a_plausible_memory_reading() {
        // The arm exists for an external application exhausting memory; if either syscall
        // failed silently, the throttle would quietly lose it.
        let m = read().expect("GlobalMemoryStatusEx + GetPerformanceInfo must succeed");
        assert!(m.avail_phys_mb > 0, "no available memory reported: {m:?}");
        assert!(m.commit_limit_mb > 0, "no commit limit reported: {m:?}");
        assert!(
            (0.0..=100.0).contains(&m.used_pct),
            "used out of range: {m:?}"
        );
        assert!(
            (0.0..=100.0).contains(&m.commit_pct),
            "commit out of range: {m:?}"
        );
        assert!(
            m.commit_charge_mb <= m.commit_limit_mb,
            "charge > limit: {m:?}"
        );
    }
}
