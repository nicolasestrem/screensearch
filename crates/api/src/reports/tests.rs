use super::*;

fn request() -> ReportPageRequest {
    ReportPageRequest {
        window: EvidenceWindow {
            from: i64::MIN,
            to: i64::MAX,
        },
        filters: ReportFilters {
            phase: None,
            outcome: None,
        },
        limit: 100,
        continuation: Some(ReportContinuation {
            snapshot: ReportSnapshot {
                max_created_at: 1_000,
                max_run_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
            },
            after: ReportPosition {
                created_at: 900,
                run_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            },
        }),
    }
}

#[test]
fn cursor_roundtrip_frozen_fields_and_optional_new_limit() {
    let phases = [
        None,
        Some(ReportRunPhase::Queued),
        Some(ReportRunPhase::Running),
        Some(ReportRunPhase::Blocked),
        Some(ReportRunPhase::Finished),
    ];
    let outcomes = [
        None,
        Some(ReportOutcome::Ok),
        Some(ReportOutcome::Partial),
        Some(ReportOutcome::NoData),
        Some(ReportOutcome::Blocked),
        Some(ReportOutcome::Failed),
    ];
    for phase in phases {
        for outcome in outcomes {
            let mut r = request();
            r.filters = ReportFilters { phase, outcome };
            let cursor = encode_report_cursor(&r).unwrap();
            assert_eq!(decode_report_cursor(&cursor, 100).unwrap(), r);
            r.limit = 1000;
            assert_eq!(
                parse_report_page(Some(&format!("cursor={cursor}&limit=1000"))).unwrap(),
                r
            );
            // Only `limit` may accompany a cursor; the frozen window/filters live inside.
            for mixed in ["from=0", "to=1", "phase=queued", "outcome=OK"] {
                assert!(parse_report_page(Some(&format!("cursor={cursor}&{mixed}"))).is_err());
            }
        }
    }
}

#[test]
fn cursor_encodes_opaque_run_ids_without_a_byte_cap_or_normalization() {
    // A long, mixed-case id round-trips byte-for-byte: no 64-byte cap, no case folding.
    let mut r = request();
    let long = format!("Run-{}-É", "x".repeat(500));
    if let Some(c) = r.continuation.as_mut() {
        c.snapshot.max_run_id = long.clone();
        c.after.run_id = long.clone();
        c.after.created_at = c.snapshot.max_created_at;
    }
    let cursor = encode_report_cursor(&r).unwrap();
    assert!(cursor.len() <= REPORT_CURSOR_MAX_BYTES);
    let decoded = decode_report_cursor(&cursor, 100).unwrap();
    let continuation = decoded.continuation.unwrap();
    assert_eq!(continuation.snapshot.max_run_id, long);
    assert_eq!(continuation.after.run_id, long);
}

#[test]
fn cursor_rejects_truncation_extensions_version_tags_and_above_max_tuples() {
    let encoded = encode_report_cursor(&request()).unwrap();
    let original = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
    for end in 0..original.len() {
        assert!(decode_report_cursor(&URL_SAFE_NO_PAD.encode(&original[..end]), 100).is_err());
    }
    let mut trailing = original.clone();
    trailing.push(0);
    assert!(decode_report_cursor(&URL_SAFE_NO_PAD.encode(trailing), 100).is_err());
    // Wrong version byte.
    let mut bytes = original.clone();
    bytes[0] = 2;
    assert!(decode_report_cursor(&URL_SAFE_NO_PAD.encode(bytes), 100).is_err());
    // Out-of-range phase tag (penultimate byte) and outcome tag (last byte).
    let n = original.len();
    let mut bytes = original.clone();
    bytes[n - 2] = 9;
    assert!(decode_report_cursor(&URL_SAFE_NO_PAD.encode(bytes), 100).is_err());
    let mut bytes = original.clone();
    bytes[n - 1] = 9;
    assert!(decode_report_cursor(&URL_SAFE_NO_PAD.encode(bytes), 100).is_err());
    // A seek tuple above the frozen maximum is invalid before it is ever encoded.
    let mut above = request();
    if let Some(c) = above.continuation.as_mut() {
        c.after.created_at = c.snapshot.max_created_at + 1;
    }
    assert!(encode_report_cursor(&above).is_err());
    let mut above_equal = request();
    if let Some(c) = above_equal.continuation.as_mut() {
        c.after.created_at = c.snapshot.max_created_at;
        c.after.run_id = "zzzzzzzz-zzzz-4zzz-8zzz-zzzzzzzzzzzz".into();
    }
    assert!(encode_report_cursor(&above_equal).is_err());
    // Empty, oversized, non-canonical base64 and a stray `+` are all rejected.
    for cursor in [
        String::new(),
        "x".repeat(REPORT_CURSOR_MAX_BYTES + 1),
        "!".into(),
        "AA+_".into(),
    ] {
        assert!(decode_report_cursor(&cursor, 100).is_err());
    }
    for limit in [0, 1001, u32::MAX] {
        assert!(decode_report_cursor(&encoded, limit).is_err());
    }
}

#[test]
fn queries_reject_duplicates_unknown_malformed_enums_and_out_of_range_limits() {
    for query in [
        "",
        "from=0",
        "to=1",
        "from=1&to=1",
        "from=2&to=1",
        "from=NaN&to=1",
        "from=0.0&to=1",
        "from=%2B0&to=1",
        "from=0&to=1&limit=0",
        "from=0&to=1&limit=1001",
        "from=0&to=1&limit=-1",
        "from=0&to=1&limit=",
        "from=0&to=1&phase=bogus",
        "from=0&to=1&outcome=ok",
        "from=0&to=1&outcome=SUCCEEDED",
        "from=0&to=1&phase=queued&phase=running",
        "from=0&to=1&from=0",
        "from=0&to=1&%66rom=0",
        "from=0&to=1&limit=1&limit=1",
        "from=0&to=1&q=x",
        "from=0&to=1&unknown=",
        "from=0&to=1&phase=%FF",
        "from=0&to=1&phase=%",
        "from=0&to=1&phase=%0g",
        "from=0&to=1&phase",
        "cursor=abc&cursor=abc",
    ] {
        assert!(parse_report_page(Some(query)).is_err(), "{query}");
    }
    let r = parse_report_page(Some(
        "from=-9223372036854775808&to=9223372036854775807&phase=finished&outcome=NO_DATA",
    ))
    .unwrap();
    assert_eq!(
        r.window,
        EvidenceWindow {
            from: i64::MIN,
            to: i64::MAX
        }
    );
    assert_eq!(r.filters.phase, Some(ReportRunPhase::Finished));
    assert_eq!(r.filters.outcome, Some(ReportOutcome::NoData));
    assert_eq!(r.limit, 100);
    assert!(r.continuation.is_none());
}
