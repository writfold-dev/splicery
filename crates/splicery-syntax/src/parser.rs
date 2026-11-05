use crate::ast::{
    Binding, Change, ChangeKind, Collection, Iteration, Piece, Recipe, Selector, Statement,
};
use crate::{Diagnostic, DiagnosticKind, Parsed, Parser, diagnostic};
use splicery_domain::{MAX_NESTING_DEPTH, MAX_RECIPE_BYTES, RecipeSpan};

impl Parser<'_> {
    fn selector(&mut self) -> Parsed<(Selector, usize)> {
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

    fn collection(&mut self) -> Parsed<Collection> {
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

    fn payload(&mut self, depth: usize) -> Parsed<(Vec<Piece>, usize)> {
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

    fn statement(&mut self) -> Parsed<Statement> {
        self.skip()?;
        if self.bytes.get(self.at) == Some(&b'@') {
            return self.binding().map(Statement::Binding);
        }
        if self.keyword("each") {
            return self.iteration().map(Statement::Iteration);
        }
        self.change().map(Statement::Change)
    }

    fn binding(&mut self) -> Parsed<Binding> {
        self.skip()?;
        let start = self.at;
        let (name, _) = self.handle()?;
        self.skip()?;
        if self.bytes.get(self.at..self.at + 2) != Some(b":=") {
            return self.fail(
                DiagnosticKind::Syntax,
                "binding requires exact @name := selector",
            );
        }
        self.at += 2;
        let (selector, end) = self.selector()?;
        Ok(Binding {
            name,
            selector,
            origin: RecipeSpan { start, end },
        })
    }

    fn change(&mut self) -> Parsed<Change> {
        self.skip()?;
        let start = self.at;
        let (kind, payload_word) = if self.eat_word("replace")? {
            (ChangeKind::Replace, "with")
        } else if self.eat_word("delete")? {
            (ChangeKind::Delete, "")
        } else if self.eat_word("before")? {
            (ChangeKind::Before, "insert")
        } else if self.eat_word("after")? {
            (ChangeKind::After, "insert")
        } else {
            return self.fail(DiagnosticKind::Syntax, "expected change");
        };
        let (selector, mut end) = self.selector()?;
        let payload = if kind == ChangeKind::Delete {
            Vec::new()
        } else {
            self.need_word(payload_word)?;
            let (pieces, payload_end) = self.payload(0)?;
            end = payload_end;
            pieces
        };
        Ok(Change {
            kind,
            selector,
            payload,
            origin: RecipeSpan { start, end },
        })
    }

    fn iteration(&mut self) -> Parsed<Iteration> {
        self.need_word("each")?;
        let start = self.at - "each".len();
        let (binder, _) = self.handle()?;
        self.need_word("in")?;
        let collection = self.collection()?;
        self.need_byte(b'{')?;
        let mut changes = Vec::new();
        while self.change_start()? {
            changes.push(self.change()?);
        }
        let end = self.need_byte(b'}')?;
        Ok(Iteration {
            binder,
            collection,
            changes,
            origin: RecipeSpan { start, end },
        })
    }

    fn change_start(&mut self) -> Parsed<bool> {
        self.skip()?;
        Ok(["replace", "delete", "before", "after"]
            .iter()
            .any(|word| self.keyword(word)))
    }

    fn statement_start(&mut self) -> Parsed<bool> {
        self.skip()?;
        Ok(self.bytes.get(self.at) == Some(&b'@')
            || self.keyword("each")
            || ["replace", "delete", "before", "after"]
                .iter()
                .any(|word| self.keyword(word)))
    }

    fn edit(&mut self) -> Parsed<Recipe> {
        self.need_word("edit")?;
        let path = self.path()?;
        let raw = self.eat_word("as")?;
        if raw {
            self.need_word("bytes")?;
        }
        self.need_byte(b'{')?;
        let mut statements = Vec::new();
        while self.statement_start()? {
            statements.push(self.statement()?);
        }
        self.need_byte(b'}')?;
        Ok(Recipe::Edit {
            path,
            raw,
            statements,
        })
    }

    fn create(&mut self) -> Parsed<Recipe> {
        self.need_word("create")?;
        let path = self.path()?;
        let raw = self.eat_word("as")?;
        if raw {
            self.need_word("bytes")?;
        }
        self.need_word("with")?;
        let (payload, _) = self.payload(0)?;
        Ok(Recipe::Create { path, raw, payload })
    }

    fn rename(&mut self) -> Parsed<Recipe> {
        self.need_word("rename")?;
        let source = self.path()?;
        self.need_word("to")?;
        let destination = self.path()?;
        Ok(Recipe::Rename {
            source,
            destination,
        })
    }

    fn remove(&mut self) -> Parsed<Recipe> {
        self.need_word("remove")?;
        Ok(Recipe::Remove { path: self.path()? })
    }

    fn fragment(&mut self) -> Parsed<Recipe> {
        if !self.statement_start()? {
            return self.fail(DiagnosticKind::Syntax, "fragment requires a statement");
        }
        let mut statements = Vec::new();
        while self.statement_start()? {
            statements.push(self.statement()?);
        }
        Ok(Recipe::Edit {
            path: String::new(),
            raw: false,
            statements,
        })
    }

    fn finish(&mut self) -> Parsed<()> {
        self.skip()?;
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            self.fail(DiagnosticKind::Syntax, "trailing input")
        }
    }
}

pub fn parse_recipe(source: &[u8], interactive_path: Option<&str>) -> Result<Recipe, Diagnostic> {
    if source.len() > MAX_RECIPE_BYTES {
        return Err(diagnostic(
            DiagnosticKind::Limit,
            0,
            64.min(source.len()),
            "recipe exceeds 256 KiB",
        ));
    }
    let text = std::str::from_utf8(source).map_err(|error| {
        let start = error.valid_up_to();
        let width = error.error_len().unwrap_or(1);
        diagnostic(
            DiagnosticKind::Encoding,
            start,
            start + width.min(source.len() - start),
            "recipe is not strict UTF-8",
        )
    })?;
    if let Some(start) = source
        .windows(3)
        .position(|window| window == b"\xef\xbb\xbf")
    {
        return Err(diagnostic(
            DiagnosticKind::Encoding,
            start,
            start + 3,
            "BOM is forbidden",
        ));
    }
    let mut parser = Parser {
        bytes: source,
        text,
        at: 0,
    };
    parser.skip()?;
    let explicit_edit = parser.keyword("edit");
    let is_fragment = !explicit_edit
        && !["create", "rename", "remove"]
            .iter()
            .any(|word| parser.keyword(word));
    let mut recipe = if explicit_edit {
        parser.edit()?
    } else if parser.keyword("create") {
        parser.create()?
    } else if parser.keyword("rename") {
        parser.rename()?
    } else if parser.keyword("remove") {
        parser.remove()?
    } else {
        parser.fragment()?
    };
    parser.finish()?;
    if is_fragment {
        let bound_path = interactive_path.map(str::to_owned).ok_or_else(|| {
            diagnostic(
                DiagnosticKind::Syntax,
                0,
                source.len(),
                "fragment requires interactive_path",
            )
        })?;
        if let Recipe::Edit { path, .. } = &mut recipe {
            *path = bound_path;
        }
    }
    if explicit_edit && interactive_path.is_some_and(|path| path != recipe.path()) {
        return Err(diagnostic(
            DiagnosticKind::Syntax,
            0,
            source.len(),
            "interactive_path conflicts with the edit path",
        ));
    }
    Ok(recipe)
}
