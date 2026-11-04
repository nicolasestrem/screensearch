//! The smart enrichment throttle (`docs/0.2.0.md` former PR5, `07` #49): a pure,
//! testable level state machine plus the kernel governor loop that drives it.
//!
//! Under sustained CPU/GPU pressure the throttle steps the worker pool down a level at a
//! time — L1 pauses `vision_tag`, L2 additionally floors `embed_text` concurrency — and
//! steps back up as pressure recovers. Capture / OCR / storage are structurally outside
//! the worker pool and never throttle (`03 §5`).
//!
//! Like `capture::trigger`, the [`ThrottleMachine`] is the testable core: **no Win32, no
//! `unsafe`, no real clock** — the [`PressureSample`] and `now_ms` are passed in. The
//! [`PressureProbe`] (Win32 in `sysmon`) and the timer live in the governor loop.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use traits::{PressureProbe, PressureSample, ThrottleAdvanced, ThrottleStatus};

use crate::events::KernelEvent;

/// How throttled enrichment currently is. The `u8` value is what the worker pool reads
/// from the shared atomic: `0` claims everything, `>= 1` pauses `vision_tag`, `2` also
/// floors `embed_text` concurrency.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ThrottleLevel {
    Normal = 0,
    High = 1,
    Sustained = 2,
}

impl ThrottleLevel {
    fn up(self) -> Self {
        match self {
            ThrottleLevel::Normal => ThrottleLevel::High,
            ThrottleLevel::High | ThrottleLevel::Sustained => ThrottleLevel::Sustained,
        }
    }
    fn down(self) -> Self {
        match self {
            ThrottleLevel::Sustained => ThrottleLevel::High,
            ThrottleLevel::High | ThrottleLevel::Normal => ThrottleLevel::Normal,
        }
    }
}

/// Hysteresis band + dwell timing for [`ThrottleMachine`]. `*_exit_pct` are kept strictly
/// below `*_enter_pct` by `AdvancedConfig::sanitized`, so a value between the two holds
/// the current level instead of flapping.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ThrottleConfig {
    pub cpu_enter_pct: f32,
    pub cpu_exit_pct: f32,
    pub gpu_enter_pct: f32,
    pub gpu_exit_pct: f32,
    pub enter_after_ms: i64,
    pub exit_after_ms: i64,
}

/// The pure throttle level machine. See the module docs for the policy it enforces.
pub(crate) struct ThrottleMachine {
    cfg: ThrottleConfig,
    level: ThrottleLevel,
    /// Pressure has been "above enter" continuously since this time (`None` otherwise).
    over_since: Option<i64>,
    /// Pressure has been "below exit" continuously since this time (`None` otherwise).
    under_since: Option<i64>,
}

impl ThrottleMachine {
    pub(crate) fn new(cfg: ThrottleConfig) -> Self {
        Self {
            cfg,
            level: ThrottleLevel::Normal,
            over_since: None,
            under_since: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn level(&self) -> ThrottleLevel {
        self.level
    }

    /// `true` when **either** resource is at/above its enter threshold (an unmonitored
    /// GPU never contributes — the truthful CPU-only path).
    fn over_enter(&self, s: &PressureSample) -> bool {
        let gpu_hot = s.gpu_monitored && s.gpu_pct.is_some_and(|g| g >= self.cfg.gpu_enter_pct);
        s.cpu_pct >= self.cfg.cpu_enter_pct || gpu_hot
    }

    /// `true` only when **both** resources are below their exit threshold (an unmonitored
    /// GPU counts as cool, so CPU recovery alone can lower the level).
    fn under_exit(&self, s: &PressureSample) -> bool {
        let cpu_cool = s.cpu_pct < self.cfg.cpu_exit_pct;
        let gpu_cool = !s.gpu_monitored || s.gpu_pct.is_none_or(|g| g < self.cfg.gpu_exit_pct);
        cpu_cool && gpu_cool
    }

    /// Feeds one pressure sample and returns the (possibly unchanged) level. Raising
    /// requires "above enter" held for `enter_after_ms`; lowering requires "below exit"
    /// held for `exit_after_ms`; a value in the hysteresis band holds the level and clears
    /// both dwell timers (so flapping never accumulates a dwell). Steps one level per
    /// dwell, so L0→L1→L2 and back are each gated.
    pub(crate) fn observe(&mut self, s: &PressureSample, now_ms: i64) -> ThrottleLevel {
        if self.over_enter(s) {
            self.under_since = None;
            let since = *self.over_since.get_or_insert(now_ms);
            if now_ms.saturating_sub(since) >= self.cfg.enter_after_ms
                && self.level != ThrottleLevel::Sustained
            {
                self.level = self.level.up();
                // Reset the dwell so the next step needs another full window.
                self.over_since = Some(now_ms);
            }
        } else if self.under_exit(s) {
            self.over_since = None;
            let since = *self.under_since.get_or_insert(now_ms);
            if now_ms.saturating_sub(since) >= self.cfg.exit_after_ms
                && self.level != ThrottleLevel::Normal
            {
                self.level = self.level.down();
                self.under_since = Some(now_ms);
            }
        } else {
            // Hysteresis band: hold the level; both dwell windows must re-accumulate.
            self.over_since = None;
            self.under_since = None;
        }
        self.level
    }
}

/// Builds a [`ThrottleConfig`] from the `file-only` advanced settings.
pub(crate) fn config_from_advanced(s: &ThrottleAdvanced) -> ThrottleConfig {
    ThrottleConfig {
        cpu_enter_pct: s.cpu_enter_pct,
        cpu_exit_pct: s.cpu_exit_pct,
        gpu_enter_pct: s.gpu_enter_pct,
        gpu_exit_pct: s.gpu_exit_pct,
        enter_after_ms: i64::from(s.enter_after_ms),
        exit_after_ms: i64::from(s.exit_after_ms),
    }
}

/// Handle to the running governor task; stop it via [`ThrottleHandle::stop`].
pub(crate) struct ThrottleHandle {
    stop: watch::Sender<bool>,
    join: Option<JoinHandle<()>>,
}

impl ThrottleHandle {
    pub(crate) async fn stop(mut self) {
        let _ = self.stop.send(true);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for ThrottleHandle {
    fn drop(&mut self) {
        // Safety net: end the loop if the handle is dropped without an explicit stop.
        let _ = self.stop.send(true);
    }
}

/// Spawns the governor loop: sample the probe on the configured cadence, drive the
/// machine, publish the level into `level` (read by the worker pool) and the latest
/// status into `status` (read by the IPC query), seed `embed_text_floor` from the
/// `file-only` advanced settings, and broadcast `ThrottleChanged`.
pub(crate) fn spawn(
    probe: Arc<dyn PressureProbe>,
    advanced: ThrottleAdvanced,
    level: Arc<AtomicU8>,
    embed_text_floor: Arc<AtomicUsize>,
    status: Arc<RwLock<ThrottleStatus>>,
    events: broadcast::Sender<KernelEvent>,
) -> ThrottleHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(governor_loop(
        probe,
        advanced,
        level,
        embed_text_floor,
        status,
        events,
        stop_rx,
    ));
    ThrottleHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

/// Floor on the sample cadence, mirroring `AdvancedConfig::sanitized`, so a bad file value
/// can't busy-loop the probe.
const MIN_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

async fn governor_loop(
    probe: Arc<dyn PressureProbe>,
    advanced: ThrottleAdvanced,
    level: Arc<AtomicU8>,
    embed_text_floor: Arc<AtomicUsize>,
    status: Arc<RwLock<ThrottleStatus>>,
    events: broadcast::Sender<KernelEvent>,
    mut stop: watch::Receiver<bool>,
) {
    // The thresholds are `file-only` (`03 §8`, 0.4.5 D6), so they are resolved once when the
    // governor starts; a changed `advanced.toml` applies on app restart.
    let interval =
        Duration::from_millis(u64::from(advanced.sample_interval_ms)).max(MIN_SAMPLE_INTERVAL);
    embed_text_floor.store(advanced.embed_text_floor.max(1) as usize, Ordering::Relaxed);
    let mut machine = ThrottleMachine::new(config_from_advanced(&advanced));
    loop {
        if wait_or_stop(&mut stop, interval).await {
            break;
        }
        let sample = probe.sample();
        let lvl = machine.observe(&sample, sample.sampled_at);
        let prev = level.swap(lvl as u8, Ordering::Relaxed);

        let snapshot = {
            let mut st = status.write().expect("throttle status lock poisoned");
            st.enabled = true;
            st.level = lvl as u8;
            st.gpu_monitored = sample.gpu_monitored;
            st.sample = Some(sample);
            *st
        };
        // Emit each tick so the Settings pressure readout stays live; a level change is
        // additionally observable via the level field consumers compare.
        let _ = events.send(KernelEvent::ThrottleChanged(snapshot));
        if prev != lvl as u8 {
            tracing::info!(
                from = prev,
                to = lvl as u8,
                "enrichment throttle level changed"
            );
        }
    }
}

/// Sleeps `dur`, returning `true` early if a stop was signalled (so the caller breaks).
async fn wait_or_stop(stop: &mut watch::Receiver<bool>, dur: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(dur) => *stop.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ThrottleConfig {
        ThrottleConfig {
            cpu_enter_pct: 85.0,
            cpu_exit_pct: 65.0,
            gpu_enter_pct: 90.0,
            gpu_exit_pct: 70.0,
            enter_after_ms: 5000,
            exit_after_ms: 8000,
        }
    }

    fn sample(cpu: f32, gpu: Option<f32>, sampled_at: i64) -> PressureSample {
        PressureSample {
            cpu_pct: cpu,
            gpu_pct: gpu,
            gpu_monitored: gpu.is_some(),
            sampled_at,
        }
    }

    #[test]
    fn normal_stays_normal_below_enter() {
        let mut m = ThrottleMachine::new(cfg());
        for t in (0..20000).step_by(1000) {
            assert_eq!(
                m.observe(&sample(50.0, Some(40.0), t), t),
                ThrottleLevel::Normal
            );
        }
    }

    #[test]
    fn enters_high_only_after_sustained_dwell() {
        let mut m = ThrottleMachine::new(cfg());
        // Above enter, but for less than the 5 s dwell → still Normal.
        assert_eq!(m.observe(&sample(95.0, None, 0), 0), ThrottleLevel::Normal);
        assert_eq!(
            m.observe(&sample(95.0, None, 4000), 4000),
            ThrottleLevel::Normal
        );
        // Dwell elapsed → High.
        assert_eq!(
            m.observe(&sample(95.0, None, 5000), 5000),
            ThrottleLevel::High
        );
    }

    #[test]
    fn escalates_to_sustained_after_continued_pressure() {
        let mut m = ThrottleMachine::new(cfg());
        assert_eq!(m.observe(&sample(95.0, None, 0), 0), ThrottleLevel::Normal);
        assert_eq!(
            m.observe(&sample(95.0, None, 5000), 5000),
            ThrottleLevel::High
        );
        // Another full dwell of continued pressure → Sustained (one level per dwell).
        assert_eq!(
            m.observe(&sample(95.0, None, 9000), 9000),
            ThrottleLevel::High
        );
        assert_eq!(
            m.observe(&sample(95.0, None, 10000), 10000),
            ThrottleLevel::Sustained
        );
        // Capped — never exceeds Sustained.
        assert_eq!(
            m.observe(&sample(95.0, None, 20000), 20000),
            ThrottleLevel::Sustained
        );
    }

    #[test]
    fn exits_one_level_at_a_time_after_recovery_dwell() {
        let mut m = ThrottleMachine::new(cfg());
        // Drive up to Sustained.
        m.observe(&sample(95.0, None, 0), 0);
        m.observe(&sample(95.0, None, 5000), 5000);
        m.observe(&sample(95.0, None, 10000), 10000);
        assert_eq!(m.level(), ThrottleLevel::Sustained);
        // Recover below exit; needs the 8 s dwell to step down one level.
        assert_eq!(
            m.observe(&sample(40.0, None, 11000), 11000),
            ThrottleLevel::Sustained
        );
        assert_eq!(
            m.observe(&sample(40.0, None, 19000), 19000),
            ThrottleLevel::High
        );
        assert_eq!(
            m.observe(&sample(40.0, None, 27000), 27000),
            ThrottleLevel::Normal
        );
    }

    #[test]
    fn hysteresis_band_holds_level() {
        let mut m = ThrottleMachine::new(cfg());
        // Get to High.
        m.observe(&sample(95.0, None, 0), 0);
        m.observe(&sample(95.0, None, 5000), 5000);
        assert_eq!(m.level(), ThrottleLevel::High);
        // 75% is between exit (65) and enter (85) → neither raises nor lowers, ever.
        for t in (6000..60000).step_by(1000) {
            assert_eq!(m.observe(&sample(75.0, None, t), t), ThrottleLevel::High);
        }
    }

    #[test]
    fn flapping_spike_does_not_trip() {
        let mut m = ThrottleMachine::new(cfg());
        // Alternating clearly-over / clearly-under samples never accumulate a dwell.
        for t in (0..60000).step_by(1000) {
            let cpu = if (t / 1000) % 2 == 0 { 95.0 } else { 40.0 };
            assert_eq!(m.observe(&sample(cpu, None, t), t), ThrottleLevel::Normal);
        }
    }

    #[test]
    fn gpu_unmonitored_is_cpu_only() {
        let mut m = ThrottleMachine::new(cfg());
        // gpu_monitored=false: even a (stale) high gpu_pct can't raise; only CPU does.
        let unmon = PressureSample {
            cpu_pct: 40.0,
            gpu_pct: Some(99.0),
            gpu_monitored: false,
            sampled_at: 0,
        };
        for t in (0..30000).step_by(1000) {
            let s = PressureSample {
                sampled_at: t,
                ..unmon
            };
            assert_eq!(m.observe(&s, t), ThrottleLevel::Normal);
        }
        // High CPU alone still throttles on the unmonitored-GPU path.
        let hot = PressureSample {
            cpu_pct: 95.0,
            gpu_pct: None,
            gpu_monitored: false,
            sampled_at: 31000,
        };
        m.observe(&hot, 31000);
        let hot2 = PressureSample {
            sampled_at: 36000,
            ..hot
        };
        assert_eq!(m.observe(&hot2, 36000), ThrottleLevel::High);
    }

    #[test]
    fn gpu_pressure_alone_can_throttle() {
        let mut m = ThrottleMachine::new(cfg());
        // Low CPU, high GPU → raises after the dwell (either resource hot trips enter).
        assert_eq!(
            m.observe(&sample(30.0, Some(95.0), 0), 0),
            ThrottleLevel::Normal
        );
        assert_eq!(
            m.observe(&sample(30.0, Some(95.0), 5000), 5000),
            ThrottleLevel::High
        );
    }

    #[test]
    fn gpu_hot_blocks_exit_even_when_cpu_cool() {
        let mut m = ThrottleMachine::new(cfg());
        // Reach High via GPU.
        m.observe(&sample(30.0, Some(95.0), 0), 0);
        m.observe(&sample(30.0, Some(95.0), 5000), 5000);
        assert_eq!(m.level(), ThrottleLevel::High);
        // CPU cools but GPU stays at 80% (above the 70% exit) → exit is blocked (held).
        for t in (6000..60000).step_by(1000) {
            assert_eq!(
                m.observe(&sample(20.0, Some(80.0), t), t),
                ThrottleLevel::High
            );
        }
    }
}
