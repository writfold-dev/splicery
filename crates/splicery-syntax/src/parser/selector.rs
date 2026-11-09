use crate::ast::{Collection, Piece, Selector};
use crate::{DiagnosticKind, Parsed, Parser};
use splicery_domain::{MAX_NESTING_DEPTH, RecipeSpan};

impl Parser<'_> {
    pub(super) fn selector(&mut self) -> Parsed<(Selector, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) == Some(&b'@') {
            let (reference, end) = self.reference()?;
            return Ok((Selector::Reference(reference), end));
        }
        if self.eat_word("one")? {
            return self.one_selector();
        }
        if self.eat_word("between")? {
            let (first, _) = self.reference()?;
            self.need_word("and")?;
            let (second, end) = self.reference()?;
            return Ok((Selector::Between { first, second }, end));
        }
        if self.eat_word("from")? {
            let (first, _) = self.reference()?;
            self.need_word("to")?;
            let (second, end) = self.reference()?;
            return Ok((Selector::FromTo { first, second }, end));
        }
        if self.eat_word("whole")? {
            let (reference, end) = self.reference()?;
            return Ok((Selector::Whole(reference), end));
        }
        if self.eat_word("range")? {
            self.need_byte(b'[')?;
            let lo = self.nat()?;
            self.need_byte(b',')?;
            let hi = self.nat()?;
            let end = self.need_byte(b')')?;
            return Ok((Selector::Range { lo, hi }, end));
        }
        self.fail(DiagnosticKind::Syntax, "expected selector")
    }

    fn one_selector(&mut self) -> Parsed<(Selector, usize)> {
        let (mut selector, mut end) = if self.eat_word("text")? {
            let (needle, end) = self.string()?;
            (
                Selector::OneText {
                    needle,
                    within: None,
                },
                end,
            )
        } else if self.eat_word("line")? {
            let (needle, end) = self.string()?;
            (
                Selector::OneLine {
                    needle,
                    within: None,
                },
                end,
            )
        } else {
            let (needle, end) = self.hex()?;
            (
                Selector::OneHex {
                    needle,
                    within: None,
                },
                end,
            )
        };
        if self.eat_word("within")? {
            let (reference, within_end) = self.reference()?;
            match &mut selector {
                Selector::OneText { within, .. }
                | Selector::OneLine { within, .. }
                | Selector::OneHex { within, .. } => *within = Some(reference),
                _ => return self.fail(DiagnosticKind::Syntax, "invalid within selector"),
            }
            end = within_end;
        }
        Ok((selector, end))
    }

    pub(super) fn collection(&mut self) -> Parsed<Collection> {
        self.skip()?;
        let start = self.at;
        if self.eat_word("frozen")? {
            let (reference, _) = self.reference()?;
            return Ok(Collection::Frozen(reference));
        }
        self.need_word("line_matches")?;
        let (pattern, _) = self.rx()?;
        let within = if self.eat_word("within")? {
            Some(self.reference()?.0)
        } else {
            None
        };
        self.need_word("expect")?;
        let expect = self.nat()?;
        Ok(Collection::LineMatches {
            pattern,
            within,
            expect,
            origin: RecipeSpan {
                start,
                end: self.at,
            },
        })
    }

    fn piece(&mut self, depth: usize) -> Parsed<(Piece, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) == Some(&b'"') {
            let (bytes, end) = self.string()?;
            return Ok((Piece::Literal(bytes), end));
        }
        if self.bytes.get(self.at..self.at.saturating_add(3)) == Some(b"<<'") {
            let (bytes, end) = self.heredoc()?;
            return Ok((Piece::Literal(bytes), end));
        }
        if self.bytes.get(self.at..self.at.saturating_add(4)) == Some(b"hex\"") {
            let (bytes, end) = self.hex()?;
            return Ok((Piece::Literal(bytes), end));
        }
        if self.eat_word("keep")? {
            self.need_byte(b'(')?;
            let (reference, _) = self.reference()?;
            let end = self.need_byte(b')')?;
            return Ok((Piece::Keep(reference), end));
        }
        if self.eat_word("lines")? {
            return self.lines_like();
        }
        if self.eat_word("indent")? {
            self.need_byte(b'(')?;
            let (prefix, _) = self.string()?;
            self.need_byte(b',')?;
            let (payload, _) = self.payload(depth + 1)?;
            let end = self.need_byte(b')')?;
            return Ok((Piece::Indent { prefix, payload }, end));
        }
        self.fail(DiagnosticKind::Syntax, "expected payload piece")
    }

    fn lines_like(&mut self) -> Parsed<(Piece, usize)> {
        self.need_word("like")?;
        let (reference, _) = self.reference()?;
        self.need_byte(b'[')?;
        let (first, _) = self.string()?;
        let mut lines = vec![first];
        while self.eat_byte(b',')? {
            self.skip()?;
            if self.bytes.get(self.at) == Some(&b']') {
                return self.fail(DiagnosticKind::Syntax, "trailing lines-like comma");
            }
            lines.push(self.string()?.0);
        }
        let end = self.need_byte(b']')?;
        Ok((Piece::LinesLike { reference, lines }, end))
    }

    pub(super) fn payload(&mut self, depth: usize) -> Parsed<(Vec<Piece>, usize)> {
        if depth > MAX_NESTING_DEPTH {
            return self.fail(DiagnosticKind::Limit, "payload nesting exceeds depth 32");
        }
        if !self.eat_byte(b'[')? {
            return self.piece(depth).map(|(piece, end)| (vec![piece], end));
        }
        let mut pieces = Vec::new();
        if self.eat_byte(b']')? {
            return Ok((pieces, self.at));
        }
        loop {
            pieces.push(self.piece(depth)?.0);
            if self.eat_byte(b',')? {
                self.skip()?;
                if self.bytes.get(self.at) == Some(&b']') {
                    return self.fail(DiagnosticKind::Syntax, "trailing payload comma");
                }
            } else {
                let end = self.need_byte(b']')?;
                return Ok((pieces, end));
            }
        }
    }
}
