use traits::{
    SegmentationParams, Session, SessionArtifact, SessionArtifactKind, SessionArtifactRole,
    SessionDetail, SessionHost, SessionKind, SessionQuery, SessionRecapRequest, SessionReference,
    UiFrameDetail, UiResumeContext, SESSION_ABSORB_MAX_MS, SESSION_FOCUS_MIN_DENSITY_FPH,
    SESSION_FOCUS_MIN_LEN_MS, SESSION_FREEZE_LOOKBACK_MS, SESSION_IDENTITY_QUALIFY_MS,
    SESSION_MEETING_GAP_MS, SESSION_MERGE_GAP_MS,
};
use ts_rs::TS;

#[test]
fn shipped_segmentation_params_pin_the_production_values() {
    let params = SegmentationParams::shipped(9_000_000, 321_000, 123_000);

    assert_eq!(params.now_ms, 9_000_000);
    assert_eq!(params.gap_close_ms, 321_000);
    assert_eq!(params.min_len_ms, 123_000);
    assert_eq!(params.merge_gap_ms, SESSION_MERGE_GAP_MS);
    assert_eq!(params.absorb_max_ms, SESSION_ABSORB_MAX_MS);
    assert_eq!(params.meeting_gap_ms, SESSION_MEETING_GAP_MS);
    assert_eq!(params.focus_min_len_ms, SESSION_FOCUS_MIN_LEN_MS);
    assert_eq!(params.focus_min_density_fph, SESSION_FOCUS_MIN_DENSITY_FPH);
    assert_eq!(params.identity_qualify_ms, SESSION_IDENTITY_QUALIFY_MS);
    assert_eq!(params.freeze_lookback_ms, SESSION_FREEZE_LOOKBACK_MS);
    assert_eq!(SESSION_FOCUS_MIN_LEN_MS, 600_000);
    assert_eq!(SESSION_FOCUS_MIN_DENSITY_FPH, 30);
    assert_eq!(SESSION_FREEZE_LOOKBACK_MS, 21_600_000);
}

#[test]
fn session_database_tokens_match_schema_eleven() {
    assert_eq!(SessionKind::Focus.as_str(), "focus");
    assert_eq!(SessionKind::Meeting.as_str(), "meeting");
    assert_eq!(SessionKind::Ai.as_str(), "ai");
    assert_eq!(SessionKind::Other.as_str(), "other");
    assert_eq!(SessionHost::Terminal.as_str(), "terminal");
    assert_eq!(SessionHost::Desktop.as_str(), "desktop");
    assert_eq!(SessionHost::Browser.as_str(), "browser");
    assert_eq!(SessionHost::Ide.as_str(), "ide");
    assert_eq!(SessionArtifactKind::Exchange.as_str(), "exchange");
    assert_eq!(SessionArtifactRole::User.as_str(), "user");
    assert_eq!(SessionArtifactRole::Agent.as_str(), "agent");
}

#[test]
fn session_ui_contract_exports_without_bigint() {
    let declarations = [
        ("SessionKind", SessionKind::inline()),
        ("SessionHost", SessionHost::inline()),
        ("SessionArtifactKind", SessionArtifactKind::inline()),
        ("SessionArtifactRole", SessionArtifactRole::inline()),
        ("Session", Session::inline()),
        ("SessionArtifact", SessionArtifact::inline()),
        ("SessionQuery", SessionQuery::inline()),
        ("SessionReference", SessionReference::inline()),
        ("SessionDetail", SessionDetail::inline()),
        ("SessionRecapRequest", SessionRecapRequest::inline()),
    ];

    for (name, declaration) in declarations {
        assert!(
            !declaration.contains("bigint"),
            "{name} exported bigint: {declaration}"
        );
    }
}

#[test]
fn external_frame_and_resume_contracts_remain_session_free() {
    let frame = traits::FrameDetail::inline();
    let resume = traits::ResumeContext::inline();
    assert!(
        !frame.contains("session"),
        "external FrameDetail drifted: {frame}"
    );
    assert!(
        !resume.contains("session"),
        "external ResumeContext drifted: {resume}"
    );

    assert!(UiFrameDetail::inline().contains("session"));
    assert!(UiResumeContext::inline().contains("session"));
}
