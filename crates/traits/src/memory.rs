//! Durable report and delivery contracts (`03 §8c`; the persisted run `kind`
//! discriminator is frozen by `03 §8e`). No credentials are carried here.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportRunPhase {
    Queued,
    Running,
    Blocked,
    Finished,
}

/// Which durable report lane a stored run belongs to (`03 §8e`). Deliberately
/// distinct from the on-demand `ReportKind` (`crate::ipc`, `03 §8b`): the
/// `weekly_synthesis` kind is the scheduled weekly layer over saved dailies and
/// never shares its token, label or filename stem with `ReportKind::Weekly`.
/// `Daily` is the default so pre-migration (schema 12) payloads keep
/// deserializing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportRunKind {
    #[default]
    Daily,
    WeeklySynthesis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportOutcome {
    Ok,
    Partial,
    NoData,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportDestination {
    Vault,
    Index,
    Telegram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum ReportDeliveryState {
    Pending,
    Sending,
    Succeeded,
    Conflict,
    Failed,
    Skipped,
    UnknownDelivery,
    Reviewed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemorySettings {
    pub enabled: bool,
    pub local_time: Option<String>,
    pub timezone: Option<String>,
    pub output_folder: Option<String>,
    pub telegram_enabled: bool,
    pub telegram_chat_id: Option<String>,
    // `serde(default)` keeps pre-G5 settings loading with weekly features off.
    #[serde(default)]
    pub weekly_enabled: bool,
    // Local `HH:MM` the weekly synthesis is due, in weekly_timezone.
    #[serde(default)]
    pub weekly_local_time: Option<String>,
    // Independent IANA zone: changing daily settings must not retime weekly runs.
    #[serde(default)]
    pub weekly_timezone: Option<String>,
    // Weekly Telegram consent is independent of telegram_enabled.
    #[serde(default)]
    pub weekly_telegram_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportScheduleState {
    pub schedule_id: String,
    #[ts(type = "number")]
    pub activated_at: i64,
    #[ts(type = "number | null")]
    pub last_window_end: Option<i64>,
    #[ts(type = "number | null")]
    pub gap_start: Option<i64>,
    #[ts(type = "number | null")]
    pub gap_end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SavedReport {
    pub markdown: String,
    pub model: Option<String>,
    #[ts(type = "number[]")]
    pub frame_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub session_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub cited_frame_ids: Vec<i64>,
    pub frames_sampled: u32,
    pub frames_summarized: u32,
    pub periods_total: u32,
    pub periods_covered: u32,
    pub passes: u32,
    pub truncated: bool,
    /// Saved daily runs this report consumed (`03 §8e`). Weekly syntheses cite them;
    /// daily reports leave it empty. Absent in pre-G2 payloads.
    #[serde(default)]
    pub daily_run_ids: Vec<String>,
}

/// Inclusive local calendar dates in the report's selected IANA time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemoryReportRange {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportRun {
    #[serde(default)]
    pub requested_range: Option<MemoryReportRange>,
    pub run_id: String,
    pub source_id: String,
    /// The persisted run-kind discriminator (`03 §8e`). Defaults to `daily` for
    /// payloads written before the schema 12 → 13 migration added the column.
    #[serde(default)]
    pub kind: ReportRunKind,
    pub schedule_id: Option<String>,
    pub revision_of: Option<String>,
    #[ts(type = "number")]
    pub window_start: i64,
    #[ts(type = "number")]
    pub window_end: i64,
    pub timezone: String,
    pub local_date: String,
    pub phase: ReportRunPhase,
    pub outcome: Option<ReportOutcome>,
    pub error: Option<ReportError>,
    pub result: Option<SavedReport>,
    pub config_snapshot: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportDelivery {
    pub id: String,
    pub run_id: String,
    pub destination: ReportDestination,
    pub state: ReportDeliveryState,
    pub revision_of: Option<String>,
    pub destination_snapshot: String,
    pub path: Option<String>,
    pub hash: Option<String>,
    pub recovery_path: Option<String>,
    pub chunk_index: Option<u32>,
    pub chunk_count: Option<u32>,
    #[ts(type = "number | null")]
    pub message_id: Option<i64>,
    pub error: Option<ReportError>,
    #[ts(type = "number | null")]
    pub next_attempt_at: Option<i64>,
    pub attempt: u32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportFileOwnership {
    pub path: String,
    pub source_id: String,
    pub run_id: String,
    pub hash: String,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ReportEvidence {
    #[ts(type = "number[]")]
    pub session_ids: Vec<i64>,
    #[ts(type = "number[]")]
    pub missing_frame_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemoryStatus {
    pub settings: MemorySettings,
    pub source_id: String,
    pub latest_run: Option<ReportRun>,
    pub missed_windows: Vec<crate::TimeRange>,
    pub worker_running: bool,
    pub worker_error: Option<ReportError>,
    pub telegram_credential_available: bool,
    // Separate latest weekly run; older status payloads remain readable.
    #[serde(default)]
    pub weekly_latest_run: Option<ReportRun>,
    // Weekly windows skipped by bounded catch-up; never claimed as covered.
    #[serde(default)]
    pub weekly_missed_windows: Vec<crate::TimeRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MemorySchedulePreview {
    pub local_date: String,
    #[ts(type = "number")]
    pub window_start: i64,
    #[ts(type = "number")]
    pub window_end: i64,
    #[ts(type = "number")]
    pub due_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum TelegramDeliveryResolution {
    Reviewed,
    Resend,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::value::{
        BoolDeserializer, Error, MapAccessDeserializer, StrDeserializer, UnitDeserializer,
    };
    use serde::de::{DeserializeSeed, MapAccess, Visitor};
    use serde::Deserialize;
    use ts_rs::TS;

    /// The six `MemorySettings` keys that existed before the weekly layer. A stored
    /// payload with exactly these keys is the compatibility case `serde(default)`
    /// must absorb.
    const LEGACY_SETTINGS_KEYS: [&str; 6] = [
        "enabled",
        "local_time",
        "timezone",
        "output_folder",
        "telegram_enabled",
        "telegram_chat_id",
    ];

    /// The seven `MemoryStatus` keys that existed before the weekly layer.
    const LEGACY_STATUS_KEYS: [&str; 7] = [
        "settings",
        "source_id",
        "latest_run",
        "missed_windows",
        "worker_running",
        "worker_error",
        "telegram_credential_available",
    ];

    struct LegacySettingsMap {
        index: usize,
    }

    impl<'de> MapAccess<'de> for LegacySettingsMap {
        type Error = Error;

        fn next_key_seed<K: DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, Self::Error> {
            let Some(key) = LEGACY_SETTINGS_KEYS.get(self.index) else {
                return Ok(None);
            };
            seed.deserialize(StrDeserializer::<Error>::new(key))
                .map(Some)
        }

        fn next_value_seed<V: DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, Self::Error> {
            let value = match self.index {
                0 => seed.deserialize(BoolDeserializer::new(true)),
                1 => seed.deserialize(SomeStr("07:30")),
                2 => seed.deserialize(SomeStr("Europe/Paris")),
                3 => seed.deserialize(UnitDeserializer::new()),
                4 => seed.deserialize(BoolDeserializer::new(true)),
                5 => seed.deserialize(SomeStr("1234")),
                _ => panic!("value requested without a key"),
            };
            self.index += 1;
            value
        }
    }

    struct LegacyStatusMap {
        index: usize,
    }

    impl<'de> MapAccess<'de> for LegacyStatusMap {
        type Error = Error;

        fn next_key_seed<K: DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, Self::Error> {
            let Some(key) = LEGACY_STATUS_KEYS.get(self.index) else {
                return Ok(None);
            };
            seed.deserialize(StrDeserializer::<Error>::new(key))
                .map(Some)
        }

        fn next_value_seed<V: DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, Self::Error> {
            let value = match self.index {
                0 => seed.deserialize(MapAccessDeserializer::new(LegacySettingsMap { index: 0 })),
                1 => seed.deserialize(StrDeserializer::<Error>::new("source-1")),
                2 | 5 => seed.deserialize(UnitDeserializer::new()),
                3 => seed.deserialize(serde::de::value::SeqDeserializer::<_, Error>::new(
                    std::iter::empty::<UnitDeserializer<Error>>(),
                )),
                4 | 6 => seed.deserialize(BoolDeserializer::new(false)),
                _ => panic!("value requested without a key"),
            };
            self.index += 1;
            value
        }
    }

    /// `Option<String>` needs `visit_some`, while serde's `StrDeserializer`
    /// forwards `deserialize_option` to `visit_str`.
    struct SomeStr(&'static str);

    impl<'de> serde::Deserializer<'de> for SomeStr {
        type Error = Error;

        fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            visitor.visit_str(self.0)
        }

        fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            visitor.visit_some(self)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct map
            struct enum identifier ignored_any
        }
    }

    #[test]
    fn pre_g5_memory_settings_default_the_weekly_fields() {
        let settings =
            MemorySettings::deserialize(MapAccessDeserializer::new(LegacySettingsMap { index: 0 }))
                .expect("pre-G5 settings payload must still deserialize");

        assert!(settings.enabled);
        assert_eq!(settings.local_time.as_deref(), Some("07:30"));
        assert_eq!(settings.timezone.as_deref(), Some("Europe/Paris"));
        assert_eq!(settings.output_folder, None);
        assert!(settings.telegram_enabled);
        assert_eq!(settings.telegram_chat_id.as_deref(), Some("1234"));
        assert!(!settings.weekly_enabled);
        assert_eq!(settings.weekly_local_time, None);
        assert_eq!(settings.weekly_timezone, None);
        assert!(!settings.weekly_telegram_enabled);
    }

    #[test]
    fn pre_g6_memory_status_defaults_the_weekly_fields() {
        let status =
            MemoryStatus::deserialize(MapAccessDeserializer::new(LegacyStatusMap { index: 0 }))
                .expect("pre-G6 status payload must still deserialize");

        assert_eq!(status.source_id, "source-1");
        assert!(status.latest_run.is_none());
        assert!(status.missed_windows.is_empty());
        assert!(!status.worker_running);
        assert!(!status.settings.weekly_enabled);
        assert_eq!(status.settings.weekly_local_time, None);
        assert_eq!(status.settings.weekly_timezone, None);
        assert!(!status.settings.weekly_telegram_enabled);
        assert_eq!(status.weekly_latest_run, None);
        assert!(status.weekly_missed_windows.is_empty());
    }

    #[test]
    fn ts_contract_carries_the_weekly_fields() {
        let settings = MemorySettings::inline();
        for field in [
            "weekly_enabled",
            "weekly_local_time",
            "weekly_timezone",
            "weekly_telegram_enabled",
        ] {
            assert!(
                settings.contains(field),
                "MemorySettings binding is missing {field}: {settings}"
            );
        }
        let status = MemoryStatus::inline();
        for field in ["weekly_latest_run", "weekly_missed_windows"] {
            assert!(
                status.contains(field),
                "MemoryStatus binding is missing {field}: {status}"
            );
        }
    }
}
