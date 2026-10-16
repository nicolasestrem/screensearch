use super::*;

fn request() -> FramePageRequest {
    FramePageRequest {
        window: EvidenceWindow {
            from: i64::MIN,
            to: i64::MAX,
        },
        filters: FrameFilters::default(),
        limit: 100,
        continuation: Some(FrameContinuation {
            snapshot: FrameSnapshot {
                history_id: "12345678-1234-1234-1234-123456789abc".into(),
                snapshot_max_frame_id: i64::MAX,
            },
            after: FramePosition {
                captured_at: -1,
                frame_id: 2,
            },
        }),
    }
}

#[test]
fn cursor_roundtrip_frozen_fields_and_optional_new_limit() {
    for app in [None, Some(""), Some(" space "), Some("é東京_\\%\0\n\"")] {
        for title in [None, Some(""), Some(" literal "), Some("é東京_\\%\0\n\"")] {
            let mut r = request();
            r.filters = FrameFilters {
                app: app.map(str::to_string),
                window_title: title.map(str::to_string),
            };
            let cursor = encode_cursor(&r).unwrap();
            assert_eq!(decode_cursor(&cursor, 100).unwrap(), r);
            r.limit = 1000;
            assert_eq!(
                parse_frames(Some(&format!("cursor={cursor}&limit=1000"))).unwrap(),
                r
            );
            for mixed in ["from=0", "to=1", "app=", "window_title="] {
                assert!(parse_frames(Some(&format!("cursor={cursor}&{mixed}"))).is_err());
            }
        }
    }
}

#[test]
fn all_accepted_filter_pairs_have_a_proven_small_wire_bound() {
    // All accepted strings occupy <=256 UTF-8 bytes; binary encoding stores each
    // exactly once, with two-byte lengths, so this bound is universal, not a
    // sampling assertion about JSON escaping. Exercise every pair of worst cases.
    let cases = [
        "\0".repeat(256),
        "\u{001f}".repeat(256),
        "\"".repeat(256),
        "\\".repeat(256),
        "é".repeat(128),
        "😀".repeat(64),
        "x".repeat(256),
    ];
    let bound = base64::encoded_len(CURSOR_BINARY_MAX, false).unwrap();
    assert_eq!(CURSOR_BINARY_MAX, 593);
    assert_eq!(bound, 791);
    assert!(bound < FRAME_CURSOR_MAX_BYTES);
    for app in &cases {
        for title in &cases {
            let mut r = request();
            r.filters = FrameFilters {
                app: Some(app.clone()),
                window_title: Some(title.clone()),
            };
            r.filters.validate().unwrap();
            let cursor = encode_cursor(&r).unwrap();
            assert_eq!(cursor.len(), bound);
            assert_eq!(decode_cursor(&cursor, r.limit).unwrap(), r);
        }
    }
}

#[test]
fn cursor_rejects_every_truncation_extensions_version_uuid_ranges_and_filters() {
    let encoded = encode_cursor(&request()).unwrap();
    let original = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
    for end in 0..original.len() {
        assert!(decode_cursor(&URL_SAFE_NO_PAD.encode(&original[..end]), 100).is_err());
    }
    let mut trailing = original.clone();
    trailing.push(0);
    assert!(decode_cursor(&URL_SAFE_NO_PAD.encode(trailing), 100).is_err());
    for (offset, replacement) in [
        (0, vec![0]),
        (0, vec![2]),
        (1, vec![b'z']),
        (9, vec![b'0']),
        (37, i64::MAX.to_be_bytes().to_vec()), // from >= to
        (45, i64::MIN.to_be_bytes().to_vec()), // to <= from
        (53, 0_i64.to_be_bytes().to_vec()),
        (53, (-1_i64).to_be_bytes().to_vec()),
        (53, 1_i64.to_be_bytes().to_vec()),    // last id above max
        (61, i64::MAX.to_be_bytes().to_vec()), // last time == to
        (69, 0_i64.to_be_bytes().to_vec()),
        (69, (-1_i64).to_be_bytes().to_vec()),
        (77, 257_u16.to_be_bytes().to_vec()),
    ] {
        let mut bytes = original.clone();
        bytes[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            decode_cursor(&URL_SAFE_NO_PAD.encode(bytes), 100).is_err(),
            "offset {offset}"
        );
    }
    let mut narrowed = request();
    narrowed.window.from = 0;
    assert!(encode_cursor(&narrowed).is_err()); // seek before from
                                                // Invalid UTF8, noncanonical ASCII uppercase and oversized string length.
    for filter_bytes in [vec![0xff], vec![b'A'], vec![b'x'; 257]] {
        let mut bytes = original[..77].to_vec();
        bytes.extend_from_slice(&(filter_bytes.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&filter_bytes);
        bytes.extend_from_slice(&u16::MAX.to_be_bytes());
        assert!(decode_cursor(&URL_SAFE_NO_PAD.encode(bytes), 100).is_err());
    }
    for cursor in [
        String::new(),
        "x".repeat(4097),
        "!".into(),
        format!("{encoded}="),
        "AA+_".into(),
    ] {
        assert!(decode_cursor(&cursor, 100).is_err());
    }
    for limit in [0, 1001, u32::MAX] {
        assert!(decode_cursor(&encoded, limit).is_err());
    }
}

#[test]
fn queries_reject_duplicates_unknown_malformed_and_validate_before_normalizing() {
    for query in [
        "",
        "from=0",
        "to=1",
        "from=1&to=1",
        "from=2&to=1",
        "from=NaN&to=1",
        "from=0.0&to=1",
        "from=1e0&to=2",
        "from=%2B0&to=1",
        "from=+0&to=1",
        "from=9223372036854775808&to=9223372036854775809",
        "from=-9223372036854775809&to=1",
        "from=0&to=1&limit=0",
        "from=0&to=1&limit=1001",
        "from=0&to=1&limit=-1",
        "from=0&to=1&limit=+1",
        "from=0&to=1&limit=4294967296",
        "from=0&to=1&limit=",
        "from=0&to=1&from=0",
        "from=0&to=1&%66rom=0",
        "from=0&to=1&limit=1&limit=1",
        "from=0&to=1&app=x&app=x",
        "from=0&to=1&window_title=x&window_title=y",
        "from=0&to=1&q=x",
        "from=0&to=1&unknown=",
        "from=0&to=1&app=%FF",
        "from=0&to=1&app=%",
        "from=0&to=1&app=%0g",
        "from=0&to=1&app",
        "cursor=abc&cursor=abc",
    ] {
        assert!(parse_frames(Some(query)).is_err(), "{query}");
    }
    let r = parse_frames(Some("from=-9223372036854775808&to=9223372036854775807&app=+VS+Code+&window_title=%C3%89%00_%25%5C")).unwrap();
    assert_eq!(
        r.window,
        EvidenceWindow {
            from: i64::MIN,
            to: i64::MAX
        }
    );
    assert_eq!(r.filters.app.as_deref(), Some(" vs code "));
    assert_eq!(r.filters.window_title.as_deref(), Some("É\0_%\\"));
    assert_eq!(r.limit, 100);
    for s in ["a".repeat(257), "é".repeat(129)] {
        assert!(parse_frames(Some(&format!("from=0&to=1&app={s}"))).is_err());
        assert!(parse_frames(Some(&format!("from=0&to=1&window_title={s}"))).is_err());
    }
    assert!(parse_frames(Some(&format!("from=0&to=1&app={}", "É".repeat(128)))).is_ok());
}
