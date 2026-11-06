use std::collections::BTreeSet;

use splicery_domain::{
    FrozenSet, LineEnding, MAX_MATCHES, OriginalImage, Profile, ResolvedCapture, SourceSpan,
};

use super::PROFILE_ID;

pub(super) fn source_span(
    image: &OriginalImage,
    lo: usize,
    hi: usize,
) -> Result<SourceSpan, String> {
    let span = SourceSpan::new(image.bytes().len(), lo, hi)
        .ok_or_else(|| "source span is reversed or outside the image".to_string())?;
    if image.profile() == Profile::Text {
        let boundary = |at| {
            at == image.bytes().len()
                || image
                    .bytes()
                    .get(at)
                    .is_some_and(|byte| byte & 0b1100_0000 != 0b1000_0000)
        };
        if !boundary(lo) || !boundary(hi) {
            return Err("source span splits a UTF-8 scalar".to_string());
        }
    }
    Ok(span)
}

pub(super) fn validate_frozen(
    frozen: &FrozenSet,
    image: Option<&OriginalImage>,
) -> Result<(), String> {
    if frozen.matcher_profile != PROFILE_ID {
        return Err("wrong frozen matcher profile".to_string());
    }
    validate_name(&frozen.name, "frozen")?;
    if frozen.members.len() > MAX_MATCHES {
        return Err("frozen member count exceeds 4096".to_string());
    }
    if let Some(scope) = frozen.scope {
        if scope.lo() > scope.hi() {
            return Err("reversed frozen scope".to_string());
        }
        if let Some(image) = image {
            source_span(image, scope.lo(), scope.hi())?;
        }
    }
    let mut previous_hi = None;
    for member in &frozen.members {
        validate_member(member, frozen.scope, previous_hi, image)?;
        previous_hi = Some(member.span.hi());
    }
    Ok(())
}

fn validate_member(
    member: &ResolvedCapture,
    scope: Option<SourceSpan>,
    previous_hi: Option<usize>,
    image: Option<&OriginalImage>,
) -> Result<(), String> {
    if member.span.lo() >= member.span.hi() {
        return Err("zero-width or reversed overall frozen member".to_string());
    }
    if previous_hi.is_some_and(|hi| hi > member.span.lo()) {
        return Err("frozen members are not sorted and non-overlapping".to_string());
    }
    if scope.is_some_and(|scope| member.span.lo() < scope.lo() || member.span.hi() > scope.hi()) {
        return Err("frozen member lies outside its scope".to_string());
    }
    if let Some(image) = image {
        source_span(image, member.span.lo(), member.span.hi())?;
        validate_line(image, member)?;
    }
    let mut names = BTreeSet::new();
    for group in &member.groups {
        validate_name(&group.name, "group")?;
        if !names.insert(group.name.as_str()) {
            return Err("duplicate capture group name".to_string());
        }
        validate_group_span(group.span, member.span, image)?;
    }
    Ok(())
}

fn validate_group_span(
    group: Option<SourceSpan>,
    member: SourceSpan,
    image: Option<&OriginalImage>,
) -> Result<(), String> {
    let Some(group) = group else {
        return Ok(());
    };
    if group.lo() > group.hi() || group.lo() < member.lo() || group.hi() > member.hi() {
        return Err("capture group lies outside its overall member".to_string());
    }
    if let Some(image) = image {
        source_span(image, group.lo(), group.hi())?;
    }
    Ok(())
}

fn validate_line(image: &OriginalImage, member: &ResolvedCapture) -> Result<(), String> {
    let Some(line) = member.line else {
        return Ok(());
    };
    let lo = member.span.lo();
    let hi = member.span.hi();
    let bytes = image.bytes();
    let body = bytes
        .get(lo..hi)
        .ok_or_else(|| "line body is outside the image".to_string())?;
    if body.contains(&b'\n') {
        return Err("line body contains a line feed".to_string());
    }
    let exact = match line {
        LineEnding::Lf => {
            bytes.get(hi) == Some(&b'\n')
                && hi.checked_sub(1).and_then(|at| bytes.get(at)) != Some(&b'\r')
        }
        LineEnding::CrLf => {
            hi.checked_add(2).and_then(|end| bytes.get(hi..end)) == Some(&b"\r\n"[..])
        }
        LineEnding::Unterminated => hi == bytes.len(),
    };
    if exact {
        Ok(())
    } else {
        Err("line terminator does not match the original image".to_string())
    }
}

fn validate_name(name: &str, kind: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || reserved(name)
    {
        return Err(format!(
            "{kind} name is not a reserved-safe ASCII handle name"
        ));
    }
    Ok(())
}

fn reserved(name: &str) -> bool {
    [
        "edit",
        "as",
        "bytes",
        "create",
        "with",
        "rename",
        "to",
        "remove",
        "replace",
        "before",
        "after",
        "insert",
        "delete",
        "each",
        "in",
        "line_matches",
        "within",
        "expect",
        "frozen",
        "one",
        "text",
        "line",
        "between",
        "and",
        "range",
        "keep",
        "lines",
        "like",
        "indent",
        "hex",
        "rx",
    ]
    .contains(&name)
}
