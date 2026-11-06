mod literal;

use crate::ast::Reference;
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

    fn fail_at<T>(
        &self,
        kind: DiagnosticKind,
        start: usize,
        end: usize,
        message: &str,
    ) -> Parsed<T> {
        Err(diagnostic(kind, start, end, message))
    }

    pub(crate) fn skip(&mut self) -> Parsed<()> {
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b' ' | b'\t' | b'\n' => self.at += 1,
                b'\r' if self.bytes.get(self.at + 1) == Some(&b'\n') => self.at += 2,
                b'\r' => return self.fail(DiagnosticKind::Syntax, "lone CR outside a literal"),
                0 => return self.fail(DiagnosticKind::Syntax, "NUL outside a literal"),
                b'#' => self.comment()?,
                _ => return Ok(()),
            }
        }
        Ok(())
    }

    fn comment(&mut self) -> Parsed<()> {
        self.at += 1;
        while let Some(&byte) = self.bytes.get(self.at) {
            if byte == 0 {
                return self.fail(DiagnosticKind::Syntax, "NUL in comment");
            }
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            self.at += 1;
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

    pub(crate) fn handle(&mut self) -> Parsed<(String, usize)> {
        self.skip()?;
        let start = self.at;
        if self.bytes.get(self.at) != Some(&b'@') {
            return self.fail(DiagnosticKind::Syntax, "expected HANDLE");
        }
        self.at += 1;
        let name_start = self.at;
        let Some((_, end)) = self.scan_name() else {
            self.at = start;
            return self.fail(DiagnosticKind::Syntax, "HANDLE requires an ASCII name");
        };
        if reserved_bytes(&self.bytes[name_start..end]) {
            return self.fail_at(DiagnosticKind::Syntax, start, end, "reserved HANDLE name");
        }
        Ok((self.text[name_start..end].to_owned(), end))
    }

    fn scan_name(&mut self) -> Option<(usize, usize)> {
        let start = self.at;
        if self
            .bytes
            .get(self.at)
            .is_none_or(|byte| !is_name_start(*byte))
        {
            return None;
        }
        self.at += 1;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| name_continue(*byte))
        {
            self.at += 1;
        }
        Some((start, self.at))
    }

    pub(crate) fn reference(&mut self) -> Parsed<(Reference, usize)> {
        let (name, mut end) = self.handle()?;
        let mut group = None;
        if self.eat_byte(b'.')? {
            self.skip()?;
            let start = self.at;
            let Some((_, group_end)) = self.scan_name() else {
                return self.fail(DiagnosticKind::Syntax, "group requires an ASCII NAME");
            };
            if reserved_bytes(&self.bytes[start..group_end]) {
                return self.fail_at(
                    DiagnosticKind::Syntax,
                    start,
                    group_end,
                    "reserved group name",
                );
            }
            group = Some(self.text[start..group_end].to_owned());
            end = group_end;
        }
        Ok((Reference { name, group }, end))
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
            return self.fail_at(
                DiagnosticKind::Syntax,
                start,
                self.at,
                "NAT has a leading zero",
            );
        }
        let mut value = 0usize;
        for byte in &self.bytes[start..self.at] {
            let digit = usize::from(byte - b'0');
            match value
                .checked_mul(10)
                .and_then(|number| number.checked_add(digit))
            {
                Some(next) => value = next,
                None => {
                    return self.fail_at(
                        DiagnosticKind::Limit,
                        start,
                        self.at,
                        "NAT exceeds usize",
                    );
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
fn reserved_bytes(name: &[u8]) -> bool {
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
    .iter()
    .any(|word| word.as_bytes() == name)
}
