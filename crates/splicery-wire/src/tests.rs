use splicery_domain::{
    CaptureGroup, FilePlan, FrozenSet, LineEnding, MAX_MATCHES, MAX_REPORT_BYTES, OriginalImage,
    Profile, ResolvedCapture, SourceSpan,
};

use super::{PROFILE_ID, capture_digest, encode_file_plan, encode_frozen, hex, parse_frozen};

fn make_image(bytes: &[u8]) -> Result<OriginalImage, String> {
    OriginalImage::new(bytes.to_vec(), Profile::Text)
        .ok_or_else(|| "invalid test image".to_string())
}

fn span(image: &OriginalImage, lo: usize, hi: usize) -> Result<SourceSpan, String> {
    SourceSpan::new(image.bytes().len(), lo, hi).ok_or_else(|| "invalid test span".to_string())
}

fn groups(image: &OriginalImage, lo: usize, hi: usize) -> Result<Vec<CaptureGroup>, String> {
    Ok(vec![
        CaptureGroup {
            name: "word".to_string(),
            span: Some(span(image, lo, hi)?),
        },
        CaptureGroup {
            name: "empty".to_string(),
            span: Some(span(image, hi, hi)?),
        },
        CaptureGroup {
            name: "absent".to_string(),
            span: None,
        },
    ])
}

fn valid_frozen(image: &OriginalImage) -> Result<FrozenSet, String> {
    let members = vec![
        ResolvedCapture {
            span: span(image, 0, 5)?,
            line: Some(LineEnding::CrLf),
            groups: groups(image, 0, 5)?,
        },
        ResolvedCapture {
            span: span(image, 7, 11)?,
            line: Some(LineEnding::Lf),
            groups: groups(image, 7, 11)?,
        },
        ResolvedCapture {
            span: span(image, 12, 17)?,
            line: Some(LineEnding::Unterminated),
            groups: groups(image, 12, 17)?,
        },
    ];
    Ok(FrozenSet {
        name: "hits".to_string(),
        matcher_profile: PROFILE_ID.to_string(),
        capture_sha256: capture_digest(image),
        scope: Some(span(image, 0, image.bytes().len())?),
        members,
    })
}

fn simple_frozen(image: &OriginalImage) -> Result<FrozenSet, String> {
    Ok(FrozenSet {
        name: "hits".to_string(),
        matcher_profile: PROFILE_ID.to_string(),
        capture_sha256: capture_digest(image),
        scope: Some(span(image, 0, image.bytes().len())?),
        members: vec![
            ResolvedCapture::plain(span(image, 0, 1)?),
            ResolvedCapture::plain(span(image, 2, 3)?),
        ],
    })
}

#[test]
fn frozen_roundtrip_preserves_absent_empty_and_line_metadata() -> Result<(), String> {
    let image = make_image(b"alpha\r\nbeta\nomega")?;
    let frozen = valid_frozen(&image)?;
    let encoded = encode_frozen(&frozen)?;
    assert!(encoded.contains("\"schema_version\":1"));
    assert!(encoded.contains("\"name\":\"hits\""));
    assert!(encoded.contains("\"line\":\"CrLf\""));
    assert!(encoded.contains("\"name\":\"empty\",\"lo\":5,\"hi\":5"));
    assert!(encoded.contains("\"name\":\"absent\",\"lo\":null,\"hi\":null"));
    assert_eq!(parse_frozen(encoded.as_bytes(), &image)?, frozen);
    assert_eq!(capture_digest(&image), image.capture_sha256());
    let mut empty = simple_frozen(&make_image(b"abc")?)?;
    empty.scope = None;
    empty.members.clear();
    let empty_json = encode_frozen(&empty)?;
    assert!(!empty_json.contains("\"scope\""));
    assert_eq!(
        parse_frozen(empty_json.as_bytes(), &make_image(b"abc")?)?,
        empty
    );
    Ok(())
}

#[test]
fn frozen_rejects_trailing_unknown_duplicate_and_missing_fields() -> Result<(), String> {
    let image = make_image(b"abc")?;
    let encoded = encode_frozen(&simple_frozen(&image)?)?;
    let unknown = encoded.replacen("\"members\":", "\"extra\":true,\"members\":", 1);
    let duplicate = encoded.replacen(
        "\"name\":\"hits\"",
        "\"name\":\"hits\",\"name\":\"other\"",
        1,
    );
    let missing_line = encoded.replace("\"line\":null,", "");
    let mut trailing = encoded.clone();
    trailing.push_str("null");
    assert!(parse_frozen(unknown.as_bytes(), &image).is_err());
    assert!(parse_frozen(duplicate.as_bytes(), &image).is_err());
    assert!(parse_frozen(missing_line.as_bytes(), &image).is_err());
    assert!(parse_frozen(trailing.as_bytes(), &image).is_err());
    assert!(parse_frozen(&[0xff], &image).is_err());
    Ok(())
}

#[test]
fn frozen_rejects_wrong_digest_profile_and_noncanonical_digest() -> Result<(), String> {
    let image = make_image(b"abc")?;
    let encoded = encode_frozen(&simple_frozen(&image)?)?;
    let wrong_image = make_image(b"abd")?;
    let wrong_profile = encoded.replacen(PROFILE_ID, "another-profile", 1);
    let wrong_digest = encoded.replacen(hex(&capture_digest(&image)).as_str(), &"0".repeat(64), 1);
    let uppercase = encoded.replacen(
        hex(&capture_digest(&image)).as_str(),
        &hex(&capture_digest(&image)).to_uppercase(),
        1,
    );
    assert!(parse_frozen(encoded.as_bytes(), &wrong_image).is_err());
    assert!(parse_frozen(wrong_profile.as_bytes(), &image).is_err());
    assert!(parse_frozen(wrong_digest.as_bytes(), &image).is_err());
    assert!(parse_frozen(uppercase.as_bytes(), &image).is_err());
    Ok(())
}

#[test]
fn frozen_rejects_bad_names_and_group_shapes() -> Result<(), String> {
    let image = make_image(b"alpha\r\nbeta\nomega")?;
    let encoded = encode_frozen(&valid_frozen(&image)?)?;
    let absent = "{\"name\":\"absent\",\"lo\":null,\"hi\":null}";
    let half_absent = "{\"name\":\"absent\",\"lo\":0,\"hi\":null}";
    let duplicate_group = encoded.replacen(
        absent,
        "{\"name\":\"word\",\"lo\":0,\"hi\":5},{\"name\":\"absent\",\"lo\":null,\"hi\":null}",
        1,
    );
    let reserved_group = encoded.replacen("\"name\":\"word\"", "\"name\":\"line\"", 1);
    let unknown_group = encoded.replacen(
        "\"name\":\"word\",\"lo\":0",
        "\"name\":\"word\",\"extra\":0,\"lo\":0",
        1,
    );
    let outside_group = encoded.replacen(
        "\"name\":\"word\",\"lo\":0,\"hi\":5",
        "\"name\":\"word\",\"lo\":0,\"hi\":6",
        1,
    );
    let reserved_name = encoded.replacen("\"name\":\"hits\"", "\"name\":\"frozen\"", 1);
    let handle_name = encoded.replacen("\"name\":\"hits\"", "\"name\":\"@hits\"", 1);
    for malformed in [
        encoded.replacen(absent, half_absent, 1),
        duplicate_group,
        reserved_group,
        unknown_group,
        outside_group,
        reserved_name,
        handle_name,
    ] {
        assert!(parse_frozen(malformed.as_bytes(), &image).is_err());
    }
    Ok(())
}

#[test]
fn frozen_rejects_scope_order_zero_width_and_wrong_terminators() -> Result<(), String> {
    let image = make_image(b"abc")?;
    let encoded = encode_frozen(&simple_frozen(&image)?)?;
    let outside_scope = encoded.replacen(
        "\"scope\":{\"lo\":0,\"hi\":3}",
        "\"scope\":{\"lo\":1,\"hi\":3}",
        1,
    );
    let zero_width = encoded.replacen(
        "{\"lo\":0,\"hi\":1,\"line\":null,\"groups\":[]}",
        "{\"lo\":0,\"hi\":0,\"line\":null,\"groups\":[]}",
        1,
    );
    let overlap = encoded.replacen(
        "{\"lo\":2,\"hi\":3,\"line\":null,\"groups\":[]}",
        "{\"lo\":0,\"hi\":2,\"line\":null,\"groups\":[]}",
        1,
    );
    let outside_span = encoded.replacen(
        "{\"lo\":2,\"hi\":3,\"line\":null,\"groups\":[]}",
        "{\"lo\":2,\"hi\":4,\"line\":null,\"groups\":[]}",
        1,
    );
    for malformed in [outside_scope, zero_width, overlap, outside_span] {
        assert!(parse_frozen(malformed.as_bytes(), &image).is_err());
    }

    let lined_image = make_image(b"alpha\r\nbeta\nomega")?;
    let lined = encode_frozen(&valid_frozen(&lined_image)?)?;
    let wrong_lf = lined.replacen("\"line\":\"CrLf\"", "\"line\":\"Lf\"", 1);
    let wrong_crlf = lined.replacen("\"line\":\"CrLf\"", "\"line\":\"Unterminated\"", 1);
    assert!(parse_frozen(wrong_lf.as_bytes(), &lined_image).is_err());
    assert!(parse_frozen(wrong_crlf.as_bytes(), &lined_image).is_err());
    Ok(())
}

#[test]
fn frozen_enforces_member_bound_before_encoding() -> Result<(), String> {
    let image = make_image(b"a")?;
    let member = ResolvedCapture::plain(span(&image, 0, 1)?);
    let frozen = FrozenSet {
        name: "hits".to_string(),
        matcher_profile: PROFILE_ID.to_string(),
        capture_sha256: capture_digest(&image),
        scope: None,
        members: vec![member; MAX_MATCHES + 1],
    };
    assert!(encode_frozen(&frozen).is_err());
    Ok(())
}

#[test]
fn file_plan_encoding_is_exact_and_bounded() -> Result<(), String> {
    let create = FilePlan::Create {
        path: "a\n".to_string(),
        content: vec![0, 0xab, 0xff],
    };
    assert_eq!(
        encode_file_plan(&create)?,
        "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"create\",\"path\":\"a\\n\",\"content_hex\":\"00abff\"}"
    );
    let rename = FilePlan::Rename {
        source: "old".to_string(),
        destination: "new".to_string(),
    };
    assert_eq!(
        encode_file_plan(&rename)?,
        "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"rename\",\"source\":\"old\",\"destination\":\"new\"}"
    );
    let remove = FilePlan::Remove {
        path: "gone".to_string(),
    };
    assert_eq!(
        encode_file_plan(&remove)?,
        "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"remove\",\"path\":\"gone\"}"
    );
    let oversized = FilePlan::Create {
        path: "large".to_string(),
        content: vec![0; MAX_REPORT_BYTES],
    };
    assert!(encode_file_plan(&oversized).is_err());
    Ok(())
}
