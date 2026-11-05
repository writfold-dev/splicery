use splicery_domain::MAX_REPORT_BYTES;
use splicery_kernel::Candidate;

pub fn parse_nat(text: &str) -> Option<usize> {
    if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.bytes().try_fold(0usize, |value, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value.checked_mul(10)?.checked_add(usize::from(byte - b'0'))
    })
}

pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let digits = text.as_bytes();
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        bytes.push(u8::try_from(high * 16 + low).ok()?);
    }
    Some(bytes)
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}

pub fn report(path: &str, candidate: &Candidate) -> Result<String, String> {
    let mut out = format!(
        "{{\"schema_version\":1,\"scope\":\"offline-preview\",\"path\":{},\"no_change\":{},\"candidate_length\":{},\"edits\":[",
        json_string(path),
        candidate.no_change(),
        candidate.bytes().len()
    );
    for (index, edit) in candidate.edits().iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"target\":[{ },{ }],\"replacement_hex\":\"{}\",\"origin\":[{},{}]}}",
            edit.target.lo(),
            edit.target.hi(),
            hex(&edit.replacement),
            edit.origin.start,
            edit.origin.end
        ));
    }
    out.push_str("]}");
    if out.len() > MAX_REPORT_BYTES {
        return Err("exact report exceeds the 16 KiB bound".to_string());
    }
    Ok(out)
}
