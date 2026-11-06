use regex::{Regex, RegexBuilder};
use splicery_domain::{
    Budget, CaptureGroup, FrozenSet, LineEnding, MAX_MATCHES, MAX_REGEX_AUTOMATON_BYTES,
    MAX_REGEX_SOURCE_BYTES, OriginalImage, Profile, ResolvedCapture, SourceSpan,
};
use std::collections::BTreeSet;

pub const PROFILE_ID: &str = "rust-regex-1.11.1/unicode-scalar-dot-posix-ascii-classes-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegexError {
    SourceTooLong,
    InvalidPattern,
    AutomatonTooLarge,
    UnsupportedConstruct,
    InvalidCaptureName,
    DuplicateCaptureName,
    WrongProfile,
    InvalidScope,
    InvalidUtf8,
    ZeroWidthMatch,
    IncompleteMatch,
    OutOfScopeMatch,
    UnexpectedMatchCount { expected: usize, actual: usize },
    TooManyMatches,
    BudgetExceeded,
}

pub fn compile_pattern(pattern: &str) -> Result<Regex, RegexError> {
    if pattern.len() > MAX_REGEX_SOURCE_BYTES {
        return Err(RegexError::SourceTooLong);
    }
    let transformed = transform_pattern(pattern)?;
    RegexBuilder::new(&transformed)
        .unicode(true)
        .nest_limit(32)
        .size_limit(MAX_REGEX_AUTOMATON_BYTES)
        .dfa_size_limit(MAX_REGEX_AUTOMATON_BYTES)
        .build()
        .map_err(|error| match error {
            regex::Error::CompiledTooBig(_) => RegexError::AutomatonTooLarge,
            _ => RegexError::InvalidPattern,
        })
}

fn transform_pattern(pattern: &str) -> Result<String, RegexError> {
    let bytes = pattern.as_bytes();
    let mut output = String::with_capacity(pattern.len());
    let mut names = BTreeSet::new();
    let mut class_depth = 0usize;
    let mut at = 0usize;
    while at < bytes.len() {
        if !bytes[at].is_ascii() {
            let width = pattern[at..]
                .chars()
                .next()
                .map(char::len_utf8)
                .ok_or(RegexError::InvalidPattern)?;
            output.push_str(&pattern[at..at + width]);
            at += width;
            continue;
        }
        match bytes[at] {
            b'\\' => {
                let next = *bytes.get(at + 1).ok_or(RegexError::InvalidPattern)?;
                if next.is_ascii_digit() || matches!(next, b'b' | b'B' | b'k' | b'g') {
                    return Err(RegexError::UnsupportedConstruct);
                }
                let replacement = match (class_depth > 0, next) {
                    (false, b'w') => Some("[[:word:]]"),
                    (false, b'd') => Some("[[:digit:]]"),
                    (false, b's') => Some("[[:space:]]"),
                    (_, b'W') => Some("[^[:word:]]"),
                    (_, b'D') => Some("[^[:digit:]]"),
                    (_, b'S') => Some("[^[:space:]]"),
                    (true, b'w') => Some("[:word:]"),
                    (true, b'd') => Some("[:digit:]"),
                    (true, b's') => Some("[:space:]"),
                    _ => None,
                };
                if let Some(replacement) = replacement {
                    output.push_str(replacement);
                    at += 2;
                } else {
                    let width = pattern[at + 1..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .ok_or(RegexError::InvalidPattern)?;
                    output.push('\\');
                    output.push_str(&pattern[at + 1..at + 1 + width]);
                    at += 1 + width;
                }
            }
            b'[' => {
                if class_depth > 0
                    && bytes.get(at + 1) == Some(&b':')
                    && let Some(relative) = pattern[at + 2..].find(":]")
                {
                    let end = at + 2 + relative + 2;
                    output.push_str(&pattern[at..end]);
                    at = end;
                    continue;
                }
                if class_depth == 0 || bytes.get(at + 1) != Some(&b']') {
                    class_depth += 1;
                }
                output.push('[');
                at += 1;
            }
            b']' => {
                class_depth = class_depth.saturating_sub(1);
                output.push(']');
                at += 1;
            }
            b'(' if class_depth == 0 && bytes.get(at + 1) == Some(&b'?') => {
                validate_group(pattern, at, &mut names)?;
                output.push('(');
                at += 1;
            }
            byte => {
                output.push(char::from(byte));
                at += 1;
            }
        }
    }
    Ok(output)
}

fn validate_group<'a>(
    pattern: &'a str,
    at: usize,
    names: &mut BTreeSet<&'a str>,
) -> Result<(), RegexError> {
    if pattern[at..].starts_with("(?:") {
        return Ok(());
    }
    if pattern[at..].starts_with("(?<=") || pattern[at..].starts_with("(?<!") {
        return Err(RegexError::UnsupportedConstruct);
    }
    if !pattern[at..].starts_with("(?<") {
        return Err(RegexError::UnsupportedConstruct);
    }
    let start = at + 3;
    let end = pattern[start..]
        .find('>')
        .map(|relative| start + relative)
        .ok_or(RegexError::InvalidCaptureName)?;
    let name = &pattern[start..end];
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid_start || !valid_rest || reserved(name) {
        return Err(RegexError::InvalidCaptureName);
    }
    if !names.insert(name) {
        return Err(RegexError::DuplicateCaptureName);
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

fn charge(budget: &mut Budget, amount: usize) -> Result<(), RegexError> {
    budget
        .charge(amount)
        .map_err(|_| RegexError::BudgetExceeded)
}

fn logical_line(bytes: &[u8], start: usize, end: usize) -> (usize, usize, LineEnding) {
    match bytes[start..end].iter().position(|byte| *byte == b'\n') {
        Some(relative) => {
            let newline = start + relative;
            if newline > start && bytes[newline - 1] == b'\r' {
                (newline - 1, newline + 1, LineEnding::CrLf)
            } else {
                (newline, newline + 1, LineEnding::Lf)
            }
        }
        None => (end, end, LineEnding::Unterminated),
    }
}

pub fn match_lines(
    image: &OriginalImage,
    name: &str,
    pattern: &str,
    scope: Option<ResolvedCapture>,
    expected: usize,
    capture_sha256: [u8; 32],
    budget: &mut Budget,
) -> Result<FrozenSet, RegexError> {
    if image.profile() != Profile::Text {
        return Err(RegexError::WrongProfile);
    }
    if pattern.len() > MAX_REGEX_SOURCE_BYTES {
        return Err(RegexError::SourceTooLong);
    }
    if expected > MAX_MATCHES {
        return Err(RegexError::TooManyMatches);
    }
    charge(budget, pattern.len())?;
    let regex = compile_pattern(pattern)?;
    charge(budget, regex.as_str().len())?;
    let bytes = image.bytes();
    let text = std::str::from_utf8(bytes).map_err(|_| RegexError::InvalidUtf8)?;
    let scope_span = scope.map(|capture| capture.span);
    let lo = scope_span.map_or(0, |span| span.lo());
    let hi = scope_span.map_or(bytes.len(), |span| span.hi());
    if scope_span.is_some_and(|span| span.capture_sha256() != image.capture_sha256())
        || hi > bytes.len()
        || !text.is_char_boundary(lo)
        || !text.is_char_boundary(hi)
    {
        return Err(RegexError::InvalidScope);
    }
    let mut members = Vec::with_capacity(expected);
    let mut cursor = 0;
    while cursor < bytes.len() {
        let (body_hi, next, ending) = logical_line(bytes, cursor, bytes.len());
        let body = text.get(cursor..body_hi).ok_or(RegexError::InvalidUtf8)?;
        let work = pattern
            .len()
            .checked_mul(body.len())
            .ok_or(RegexError::BudgetExceeded)?;
        charge(budget, work)?;
        for captures in regex.captures_iter(body) {
            let whole = captures.get(0).ok_or(RegexError::IncompleteMatch)?;
            charge(budget, whole.len())?;
            charge_named_captures(&regex, &captures, budget)?;
            let span = capture_span(image, cursor, body.len(), 0, bytes.len(), &whole)?;
            if span.lo() < lo || span.hi() > hi {
                continue;
            }
            if whole.start() == whole.end() {
                return Err(RegexError::ZeroWidthMatch);
            }
            if members.len() >= MAX_MATCHES {
                return Err(RegexError::TooManyMatches);
            }
            let mut groups = Vec::with_capacity(regex.capture_names().flatten().count());
            for (index, group_name) in regex.capture_names().enumerate().skip(1) {
                let Some(group_name) = group_name else {
                    continue;
                };
                let found = captures.get(index);
                let group_span = match found {
                    Some(found) => {
                        if found.start() < whole.start() || found.end() > whole.end() {
                            return Err(RegexError::IncompleteMatch);
                        }
                        Some(capture_span(image, cursor, body.len(), lo, hi, &found)?)
                    }
                    None => None,
                };
                groups.push(CaptureGroup {
                    name: group_name.to_owned(),
                    span: group_span,
                });
            }
            let line = (whole.start() == 0 && whole.end() == body.len()).then_some(ending);
            members.push(ResolvedCapture { span, line, groups });
        }
        cursor = next;
    }
    if members.len() != expected {
        return Err(RegexError::UnexpectedMatchCount {
            expected,
            actual: members.len(),
        });
    }
    Ok(FrozenSet {
        name: name.to_owned(),
        matcher_profile: PROFILE_ID.to_owned(),
        capture_sha256,
        scope: scope_span,
        members,
    })
}

fn charge_named_captures(
    regex: &Regex,
    captures: &regex::Captures<'_>,
    budget: &mut Budget,
) -> Result<(), RegexError> {
    for (index, name) in regex.capture_names().enumerate().skip(1) {
        if name.is_some()
            && let Some(found) = captures.get(index)
        {
            charge(budget, found.len())?;
        }
    }
    Ok(())
}

fn capture_span(
    image: &OriginalImage,
    body_lo: usize,
    body_len: usize,
    scope_lo: usize,
    scope_hi: usize,
    found: &regex::Match<'_>,
) -> Result<SourceSpan, RegexError> {
    if found.end() > body_len {
        return Err(RegexError::IncompleteMatch);
    }
    let lo = body_lo
        .checked_add(found.start())
        .ok_or(RegexError::OutOfScopeMatch)?;
    let hi = body_lo
        .checked_add(found.end())
        .ok_or(RegexError::OutOfScopeMatch)?;
    if lo < scope_lo || hi > scope_hi {
        return Err(RegexError::OutOfScopeMatch);
    }
    SourceSpan::new(image, lo, hi).ok_or(RegexError::OutOfScopeMatch)
}

#[cfg(test)]
mod tests;
