use crate::{DiagnosticKind, Parsed, Parser, diagnostic};

impl<'a> Parser<'a> {
    pub(crate) fn fail<T>(&self, kind: DiagnosticKind, message: &str) -> Parsed<T> {
        let end = if self.at < self.bytes.len() {
            self.at + 1
        } else {
            self.at
        };
        Err(diagnostic(kind, self.at, end, message))
    }

    pub(crate) fn unsupported<T>(&self, message: &str) -> Parsed<T> {
        self.fail(DiagnosticKind::Unsupported, message)
    }

    pub(crate) fn skip(&mut self) -> Parsed<()> {
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b' ' | b'\t' | b'\n' => self.at += 1,
                b'\r' if self.bytes.get(self.at + 1) == Some(&b'\n') => self.at += 2,
                b'\r' => return self.fail(DiagnosticKind::Syntax, "lone CR outside a literal"),
                0 => return self.fail(DiagnosticKind::Syntax, "NUL outside a literal"),
                b'#' => {
                    self.at += 1;
                    while let Some(&next) = self.bytes.get(self.at) {
                        if next == 0 {
                            return self.fail(DiagnosticKind::Syntax, "NUL in comment");
                        }
                        if next == b'\n' || next == b'\r' {
                            break;
                        }
                        self.at += 1;
                    }
                }
                _ => return Ok(()),
            }
        }
        Ok(())
    }

    pub(crate) fn keyword(&self, word: &str) -> bool {
        let end = self.at.saturating_add(word.len());
        self.bytes.get(self.at..end) == Some(word.as_bytes())
            && self.bytes.get(end).is_none_or(|byte| !name_continue(*byte))
    }

    pub(crate) fn eat_word(&mut self, word: &str) -> Parsed<bool> {
        self.skip()?;
        if self.keyword(word) {
            self.at += word.len();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn need_word(&mut self, word: &str) -> Parsed<()> {
        if self.eat_word(word)? {
            Ok(())
        } else {
            self.fail(DiagnosticKind::Syntax, "unexpected keyword")
        }
    }

    pub(crate) fn eat_byte(&mut self, wanted: u8) -> Parsed<bool> {
        self.skip()?;
        if self.bytes.get(self.at) == Some(&wanted) {
            self.at += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn need_byte(&mut self, wanted: u8) -> Parsed<usize> {
        if self.eat_byte(wanted)? {
            Ok(self.at)
        } else {
            self.fail(DiagnosticKind::Syntax, "expected punctuation")
        }
    }

    pub(crate) fn string(&mut self) -> Parsed<(Vec<u8>, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) != Some(&b'"') {
            return self.fail(DiagnosticKind::Syntax, "expected STRING");
        }
        self.at += 1;
        let mut value = Vec::new();
        while let Some(&byte) = self.bytes.get(self.at) {
            if byte == b'"' {
                self.at += 1;
                return Ok((value, self.at));
            }
            if byte == b'\\' {
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
            } else if byte == b'\n' || byte == b'\r' || byte < 0x20 || byte == 0x7f {
                return self.fail(DiagnosticKind::Syntax, "unescaped control in STRING");
            } else if byte < 0x80 {
                value.push(byte);
                self.at += 1;
            } else {
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
                value.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                self.at += ch.len_utf8();
            }
        }
        self.fail(DiagnosticKind::Syntax, "unterminated STRING")
    }

    pub(crate) fn path(&mut self) -> Parsed<String> {
        String::from_utf8(self.string()?.0).map_err(|_| {
            diagnostic(
                DiagnosticKind::Encoding,
                self.at,
                self.at,
                "path is not UTF-8",
            )
        })
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
            if self.bytes.get(self.at..after) == Some(tag) {
                let newline = self.bytes.get(after..after + 2) == Some(b"\r\n");
                if self.bytes.get(after) == Some(&b'\n') || newline || after == self.bytes.len() {
                    let body = self.bytes[content_start..self.at].to_vec();
                    self.at = after
                        + if newline {
                            2
                        } else if after < self.bytes.len() {
                            1
                        } else {
                            0
                        };
                    return Ok((body, self.at));
                }
            }
            match self.bytes[self.at..].iter().position(|byte| *byte == b'\n') {
                Some(width) => self.at += width + 1,
                None => return self.fail(DiagnosticKind::Syntax, "unterminated HEREDOC"),
            }
        }
    }

    pub(crate) fn handle(&mut self) -> Parsed<(String, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) != Some(&b'@') {
            return self.fail(DiagnosticKind::Syntax, "expected HANDLE");
        }
        let start = self.at;
        self.at += 1;
        let name_start = self.at;
        if self
            .bytes
            .get(self.at)
            .is_none_or(|byte| !is_name_start(*byte))
        {
            return self.fail(DiagnosticKind::Syntax, "HANDLE requires an ASCII name");
        }
        self.at += 1;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| name_continue(*byte))
        {
            self.at += 1;
        }
        let name = &self.text[name_start..self.at];
        if reserved(name) {
            self.at = start;
            return self.fail(DiagnosticKind::Syntax, "reserved HANDLE name");
        }
        Ok((name.to_owned(), self.at))
    }

    pub(crate) fn reference(&mut self) -> Parsed<(String, usize)> {
        let (name, end) = self.handle()?;
        if self.eat_byte(b'.')? {
            return self.unsupported("regex group references are outside this profile");
        }
        Ok((name, end))
    }

    pub(crate) fn nat(&mut self) -> Parsed<usize> {
        self.skip()?;
        let start = self.at;
        if self
            .bytes
            .get(self.at)
            .is_none_or(|byte| !byte.is_ascii_digit())
        {
            return self.fail(DiagnosticKind::Syntax, "expected NAT");
        }
        self.at += 1;
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.at - start > 1 && self.bytes[start] == b'0' {
            self.at = start;
            return self.fail(DiagnosticKind::Syntax, "NAT has a leading zero");
        }
        let mut value = 0usize;
        for byte in &self.bytes[start..self.at] {
            let digit = usize::from(byte - b'0');
            match value.checked_mul(10).and_then(|n| n.checked_add(digit)) {
                Some(next) => value = next,
                None => {
                    return Err(diagnostic(
                        DiagnosticKind::Limit,
                        start,
                        self.at,
                        "NAT exceeds usize",
                    ));
                }
            }
        }
        Ok(value)
    }
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}
fn name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
