use splicery_domain::{FrozenSet, MAX_MATCHES, OriginalImage, Profile, ResolvedCapture};

mod dto;
mod validation;

use dto::{FrozenDto, LineDto, MemberDto};
use validation::{source_span, validate_frozen};

pub const PROFILE_ID: &str = "rust-regex-1.11.1/unicode-scalar-dot-posix-ascii-classes-v1";
const MAX_FROZEN_BYTES: usize = 1024 * 1024;

pub fn capture_digest(image: &OriginalImage) -> [u8; 32] {
    image.capture_sha256()
}

pub fn parse_frozen(bytes: &[u8], image: &OriginalImage) -> Result<FrozenSet, String> {
    if bytes.len() > MAX_FROZEN_BYTES {
        return Err("frozen JSON exceeds the 1 MiB bound".to_string());
    }
    let dto: FrozenDto =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid frozen JSON: {error}"))?;
    if dto.schema_version != 1 {
        return Err("unsupported frozen schema_version".to_string());
    }
    if dto.profile != PROFILE_ID {
        return Err("wrong frozen matcher profile".to_string());
    }
    if dto.members.len() > MAX_MATCHES {
        return Err("frozen member count exceeds 4096".to_string());
    }
    let capture_sha256 = decode_digest(&dto.capture_sha256)?;
    if capture_sha256 != capture_digest(image) {
        return Err("frozen capture digest does not match the original image".to_string());
    }
    if image.profile() != Profile::Text {
        return Err("frozen regex captures require a text image".to_string());
    }
    let scope = dto
        .scope
        .map(|span| source_span(image, span.lo, span.hi))
        .transpose()?;
    let mut members = Vec::with_capacity(dto.members.len());
    for member in dto.members {
        members.push(decode_member(member, image)?);
    }
    let frozen = FrozenSet {
        name: dto.name,
        matcher_profile: dto.profile,
        capture_sha256,
        scope,
        members,
    };
    validate_frozen(&frozen, Some(image))?;
    Ok(frozen)
}

fn decode_digest(text: &str) -> Result<[u8; 32], String> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("capture_sha256 must be exactly 64 lowercase hex digits".to_string());
    }
    let decoded = super::parse_hex(text).ok_or_else(|| "invalid capture_sha256".to_string())?;
    <[u8; 32]>::try_from(decoded).map_err(|_| "invalid capture_sha256 length".to_string())
}

fn decode_member(member: MemberDto, image: &OriginalImage) -> Result<ResolvedCapture, String> {
    let span = source_span(image, member.lo, member.hi)?;
    let mut groups = Vec::with_capacity(member.groups.len());
    for group in member.groups {
        let span = match (group.lo, group.hi) {
            (None, None) => None,
            (Some(lo), Some(hi)) => Some(source_span(image, lo, hi)?),
            _ => return Err("group lo and hi must both be null or both be integers".to_string()),
        };
        groups.push(splicery_domain::CaptureGroup {
            name: group.name,
            span,
        });
    }
    Ok(ResolvedCapture {
        span,
        line: member.line.map(LineDto::into_domain),
        groups,
    })
}

pub fn encode_frozen(frozen: &FrozenSet) -> Result<String, String> {
    validate_frozen(frozen, None)?;
    let view = dto::FrozenRef::new(frozen);
    dto::bounded_json(&view)
}
