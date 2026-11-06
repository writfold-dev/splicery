use super::{is_name_start, name_continue, reserved_bytes};
use crate::{DiagnosticKind, Parsed, Parser};
use splicery_domain::MAX_REGEX_SOURCE_BYTES;

impl<'a> Parser<'a> {
    pub(crate) fn string(&mut self) -> Parsed<(Vec<u8>, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) != Some(&b'"') {
            return self.fail(DiagnosticKind::Syntax, "expected STRING");
        }
        self.at += 1;
        let mut value = Vec::new();
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok((value, self.at));
                }
                b'\\' => self.string_escape(&mut value)?,
                0..=0x1f | 0x7f => {
                    return self.fail(DiagnosticKind::Syntax, "unescaped control in STRING");
                }
                0x20..=0x7e => {
                    value.push(byte);
                    self.at += 1;
                }
                _ => self.string_unicode(&mut value)?,
            }
        }
        self.fail(DiagnosticKind::Syntax, "unterminated STRING")
    }

    fn string_escape(&mut self, value: &mut Vec<u8>) -> Parsed<()> {
        self.at += 1;
        let Some(&escape) = self.bytes.get(self.at) else {
            return self.fail(DiagnosticKind::Syntax, "unterminated STRING");
        };
        let decoded = match escape {
            b'"' | b'\\' => escape,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            _ => return self.fail(DiagnosticKind::Syntax, "invalid STRING escape"),
        };
        value.push(decoded);
        self.at += 1;
        Ok(())
    }

    fn string_unicode(&mut self, value: &mut Vec<u8>) -> Parsed<()> {
        let Some(ch) = self
            .text
            .get(self.at..)
            .and_then(|tail| tail.chars().next())
        else {
            return self.fail(DiagnosticKind::Encoding, "invalid UTF-8 in STRING");
        };
        if ch.is_control() {
            return self.fail(DiagnosticKind::Syntax, "unescaped control in STRING");
        }
        let mut encoded = [0; 4];
        value.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        self.at += ch.len_utf8();
        Ok(())
    }

    pub(crate) fn hex(&mut self) -> Parsed<(Vec<u8>, usize)> {
        self.skip()?;
        if self.bytes.get(self.at..self.at.saturating_add(4)) != Some(b"hex\"") {
            return self.fail(DiagnosticKind::Syntax, "expected HEX");
        }
        self.at += 4;
        let mut value = Vec::new();
        let mut high = None;
        while let Some(&byte) = self.bytes.get(self.at) {
            if byte == b'"' {
                if high.is_some() {
                    return self.fail(DiagnosticKind::Syntax, "odd number of HEX digits");
                }
                self.at += 1;
                return Ok((value, self.at));
            }
            let Some(digit) = hex_digit(byte) else {
                return self.fail(DiagnosticKind::Syntax, "invalid HEX digit");
            };
            if let Some(first) = high.take() {
                value.push(first * 16 + digit);
            } else {
                high = Some(digit);
            }
            self.at += 1;
        }
        self.fail(DiagnosticKind::Syntax, "unterminated HEX")
    }

    pub(crate) fn rx(&mut self) -> Parsed<(String, usize)> {
        self.skip()?;
        if self.bytes.get(self.at..self.at.saturating_add(3)) != Some(b"rx\"") {
            return self.fail(DiagnosticKind::Syntax, "expected RX");
        }
        self.at += 3;
        let start = self.at;
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b'"' => {
                    let end = self.at;
                    if end - start > MAX_REGEX_SOURCE_BYTES {
                        return self.fail_at(DiagnosticKind::Limit, start, end, "RX exceeds 4 KiB");
                    }
                    self.check_regex_groups(start, end)?;
                    self.at += 1;
                    return Ok((self.text[start..end].to_owned(), self.at));
                }
                b'\n' | b'\r' => {
                    return self.fail(DiagnosticKind::Syntax, "RX must be single-line");
                }
                _ => self.at += 1,
            }
        }
        self.fail(DiagnosticKind::Syntax, "unterminated RX")
    }

    fn check_regex_groups(&self, start: usize, end: usize) -> Parsed<()> {
        let mut at = start;
        let mut in_class = false;
        while at < end {
            match self.bytes[at] {
                b'\\' => at = (at + 2).min(end),
                b'[' if !in_class => {
                    in_class = true;
                    at += 1;
                }
                b']' if in_class => {
                    in_class = false;
                    at += 1;
                }
                b'(' if !in_class && self.bytes.get(at..at + 3) == Some(b"(?<") => {
                    let name_start = at + 3;
                    let mut cursor = name_start;
                    if self
                        .bytes
                        .get(cursor)
                        .is_some_and(|byte| is_name_start(*byte))
                    {
                        cursor += 1;
                        while self
                            .bytes
                            .get(cursor)
                            .is_some_and(|byte| name_continue(*byte))
                        {
                            cursor += 1;
                        }
                        if self.bytes.get(cursor) == Some(&b'>')
                            && reserved_bytes(&self.bytes[name_start..cursor])
                        {
                            return self.fail_at(
                                DiagnosticKind::Syntax,
                                name_start,
                                cursor,
                                "reserved regex group name",
                            );
                        }
                    }
                    at += 1;
                }
                _ => at += 1,
            }
        }
        Ok(())
    }

    pub(crate) fn heredoc(&mut self) -> Parsed<(Vec<u8>, usize)> {
        self.skip()?;
        if self.bytes.get(self.at..self.at.saturating_add(3)) != Some(b"<<'") {
            return self.fail(DiagnosticKind::Syntax, "HEREDOC begins with <<'TAG'");
        }
        self.at += 3;
        let tag_start = self.at;
        if self
            .bytes
            .get(self.at)
            .is_none_or(|byte| !is_name_start(*byte))
        {
            return self.fail(DiagnosticKind::Syntax, "invalid HEREDOC tag");
        }
        self.at += 1;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| name_continue(*byte))
        {
            self.at += 1;
        }
        if self.at - tag_start > 64 {
            return self.fail(DiagnosticKind::Limit, "HEREDOC tag exceeds 64 bytes");
        }
        let tag = &self.bytes[tag_start..self.at];
        if self.bytes.get(self.at) != Some(&b'\'') {
            return self.fail(DiagnosticKind::Syntax, "unterminated HEREDOC tag");
        }
        self.at += 1;
        if self.bytes.get(self.at..self.at + 2) == Some(b"\r\n") {
            self.at += 2;
        } else if self.bytes.get(self.at) == Some(&b'\n') {
            self.at += 1;
        } else {
            return self.fail(DiagnosticKind::Syntax, "HEREDOC opener requires LF or CRLF");
        }
        let content_start = self.at;
        loop {
            let after = self.at + tag.len();
            let newline = self.bytes.get(after..after + 2) == Some(b"\r\n");
            if self.bytes.get(self.at..after) == Some(tag)
                && (newline || self.bytes.get(after) == Some(&b'\n') || after == self.bytes.len())
            {
                let body = self.bytes[content_start..self.at].to_vec();
                self.at = after
                    + if newline {
                        2
                    } else {
                        usize::from(after < self.bytes.len())
                    };
                return Ok((body, self.at));
            }
            match self.bytes[self.at..].iter().position(|byte| *byte == b'\n') {
                Some(width) => self.at += width + 1,
                None => return self.fail(DiagnosticKind::Syntax, "unterminated HEREDOC"),
            }
        }
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
