//! `sysmon` — the Windows-native system-pressure probe for the smart enrichment throttle
//! (`03 §5/§8`, `docs/0.2.0.md` former PR5).
//!
//! Reads whole-machine CPU busy % (`GetSystemTimes`), GPU utilization (PDH
//! `\GPU Engine(*)\Utilization Percentage`) and memory pressure (`GlobalMemoryStatusEx`
//! physical usage + `GetPerformanceInfo` host commit charge) and exposes them through
//! [`traits::PressureProbe`]. The kernel forbids `unsafe`, so this crate owns the only
//! Win32 here and the composition root injects it as `dyn PressureProbe` — the same seam
//! as `capture`'s `IdleSource` (`03 §2`).
//!
//! The probe is **stateful** (CPU busy % is a delta between two `GetSystemTimes` readings;
//! GPU is a PDH rate counter needing repeated collects on one open query) and
//! **infallible by contract** — a failed read degrades to a truthful sample, never an
//! error.

use std::sync::{Arc, Mutex};

use traits::{PressureProbe, PressureSample};

mod cpu;
mod gpu;
mod mem;

use cpu::CpuTimes;
use gpu::GpuProbe;

/// The native CPU+GPU+memory pressure probe. Construct once via [`native_probe`]; sampling
/// is cheap (system counters, no allocations) and serialized behind a `Mutex` since the
/// trait samples through `&self` while the CPU and GPU readings are stateful.
struct NativeProbe {
    state: Mutex<ProbeState>,
    /// Cached so [`PressureProbe::gpu_monitored`] answers without locking or sampling.
    gpu_monitored: bool,
}

struct ProbeState {
    prev_cpu: Option<CpuTimes>,
    /// Last good CPU %, returned when a single `GetSystemTimes` delta can't be formed.
    last_cpu_pct: f32,
    gpu: GpuProbe,
}

impl NativeProbe {
    fn new() -> Self {
        let gpu = GpuProbe::new();
        let gpu_monitored = gpu.monitored();
        // Prime the CPU baseline so the first real sample can form a delta.
        let prev_cpu = cpu::read_times();
        NativeProbe {
            state: Mutex::new(ProbeState {
                prev_cpu,
                last_cpu_pct: 0.0,
                gpu,
            }),
            gpu_monitored,
        }
    }
}

impl PressureProbe for NativeProbe {
    fn sample(&self) -> PressureSample {
        let mut st = self.state.lock().expect("sysmon probe lock poisoned");
        let cpu_pct = match cpu::read_times() {
            Some(now) => {
                let pct = st
                    .prev_cpu
                    .and_then(|prev| cpu::busy_pct(prev, now))
                    .unwrap_or(st.last_cpu_pct);
                st.prev_cpu = Some(now);
                st.last_cpu_pct = pct;
                pct
            }
            None => st.last_cpu_pct,
        };
        let gpu_pct = st.gpu.sample();
        let gpu_monitored = st.gpu.monitored();
        // Memory readings are stateless (two cheap syscalls), so they are read fresh every
        // tick. A failure reports the honest "not monitored" state instead of a zero.
        let memory = mem::read();
        PressureSample {
            cpu_pct,
            gpu_pct,
            gpu_monitored,
            mem_used_pct: memory.map_or(0.0, |m| m.used_pct),
            commit_pct: memory.map_or(0.0, |m| m.commit_pct),
            avail_phys_mb: memory.map_or(0, |m| m.avail_phys_mb),
            commit_charge_mb: memory.map_or(0, |m| m.commit_charge_mb),
            commit_limit_mb: memory.map_or(0, |m| m.commit_limit_mb),
            mem_monitored: memory.is_some(),
            sampled_at: now_ms(),
        }
    }

    fn gpu_monitored(&self) -> bool {
        self.gpu_monitored
    }

    fn mem_monitored(&self) -> bool {
        // No latch: the memory counters are either readable right now or they are not.
        mem::read().is_some()
    }
}

/// Builds the native pressure probe (opens PDH + primes the CPU baseline) and returns it
/// as a `dyn PressureProbe` for the kernel. Logs once whether the GPU is monitored.
pub fn native_probe() -> Arc<dyn PressureProbe> {
    let probe = NativeProbe::new();
    tracing::info!(
        gpu_monitored = probe.gpu_monitored,
        "sysmon pressure probe initialized"
    );
    Arc::new(probe)
}

/// Current unix time in milliseconds (the kernel's clock unit, `03 §4`).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_well_formed() {
        // The probe is infallible: a sample always lands with CPU in range and a
        // gpu_monitored flag consistent with gpu_pct (Some iff monitored is plausible).
        let probe = native_probe();
        let s = probe.sample();
        assert!(
            (0.0..=100.0).contains(&s.cpu_pct),
            "cpu out of range: {s:?}"
        );
        if let Some(g) = s.gpu_pct {
            assert!((0.0..=100.0).contains(&g), "gpu out of range: {s:?}");
        }
        // The memory arm is infallible too: either a plausible reading or the honest
        // "not monitored" state with zeroed fields, never a partial one.
        assert!(
            (0.0..=100.0).contains(&s.mem_used_pct),
            "mem out of range: {s:?}"
        );
        assert!(
            (0.0..=100.0).contains(&s.commit_pct),
            "commit out of range: {s:?}"
        );
        if s.mem_monitored {
            assert!(s.commit_limit_mb > 0, "monitored with no limit: {s:?}");
            assert!(
                s.commit_charge_mb <= s.commit_limit_mb,
                "charge exceeds limit: {s:?}"
            );
        } else {
            assert_eq!(s.commit_limit_mb, 0, "unmonitored with a limit: {s:?}");
        }
        // gpu_monitored() must agree with the per-sample flag.
        assert_eq!(probe.gpu_monitored(), s.gpu_monitored);
    }
}
