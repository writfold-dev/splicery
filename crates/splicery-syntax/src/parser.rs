use crate::ast::{Binding, Change, ChangeKind, Piece, Recipe, Selector, Statement};
use crate::{Diagnostic, DiagnosticKind, Parsed, Parser, diagnostic};
use splicery_domain::{MAX_RECIPE_BYTES, RecipeSpan};

impl Parser<'_> {
    fn selector(&mut self) -> Parsed<(Selector, usize)> {
        self.skip()?;
        if self.bytes.get(self.at) == Some(&b'@') {
            let (name, end) = self.reference()?;
            return Ok((Selector::Reference(name), end));
        }
        if self.eat_word("one")? {
            let (mut selector, mut end) = if self.eat_word("text")? {
                let (needle, end) = self.string()?;
                (
                    Selector::OneText {
                        needle,
                        within: None,
                    },
                    end,
                )
            } else if self.keyword("line") {
                return self.unsupported("one line is outside this profile");
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
                let (name, within_end) = self.reference()?;
                match &mut selector {
                    Selector::OneText { within, .. } | Selector::OneHex { within, .. } => {
                        *within = Some(name)
                    }
                    _ => return self.fail(DiagnosticKind::Syntax, "invalid within selector"),
                }
                end = within_end;
            }
            return Ok((selector, end));
        }
        if self.keyword("between") {
            return self.unsupported("between selectors are outside this profile");
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

    fn piece(&mut self) -> Parsed<(Piece, usize)> {
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
            let (name, _) = self.reference()?;
            let end = self.need_byte(b')')?;
            return Ok((Piece::Keep(name), end));
        }
        if self.keyword("lines") || self.keyword("indent") {
            return self.unsupported("lines like and indent payloads are outside this profile");
        }
        self.fail(DiagnosticKind::Syntax, "expected payload piece")
    }

    fn payload(&mut self) -> Parsed<(Vec<Piece>, usize)> {
        if !self.eat_byte(b'[')? {
            return self.piece().map(|(piece, end)| (vec![piece], end));
        }
        let mut pieces = Vec::new();
        if self.eat_byte(b']')? {
            return Ok((pieces, self.at));
        }
        loop {
            let (piece, _) = self.piece()?;
            pieces.push(piece);
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
        let start = self.at;
        if self.bytes.get(self.at) == Some(&b'@') {
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
            return Ok(Statement::Binding(Binding {
                name,
                selector,
                origin: RecipeSpan { start, end },
            }));
        }
        let (kind, payload_word) = if self.eat_word("replace")? {
            (ChangeKind::Replace, "with")
        } else if self.eat_word("delete")? {
            (ChangeKind::Delete, "")
        } else if self.eat_word("before")? {
            (ChangeKind::Before, "insert")
        } else if self.eat_word("after")? {
            (ChangeKind::After, "insert")
        } else {
            return self.fail(DiagnosticKind::Syntax, "expected statement");
        };
        let (selector, mut end) = self.selector()?;
        let payload = if kind == ChangeKind::Delete {
            Vec::new()
        } else {
            self.need_word(payload_word)?;
            let (pieces, payload_end) = self.payload()?;
            end = payload_end;
            pieces
        };
        Ok(Statement::Change(Change {
            kind,
            selector,
            payload,
            origin: RecipeSpan { start, end },
        }))
    }

    fn statement_start(&mut self) -> Parsed<bool> {
        self.skip()?;
        Ok(self.bytes.get(self.at) == Some(&b'@')
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
        if self.keyword("each") {
            return self.unsupported("iteration is outside this profile");
        }
        self.need_byte(b'}')?;
        Ok(Recipe {
            path,
            raw,
            statements,
        })
    }

    fn fragment(&mut self) -> Parsed<Recipe> {
        if !self.statement_start()? {
            if self.keyword("each") {
                return self.unsupported("iteration is outside this profile");
            }
            return self.fail(DiagnosticKind::Syntax, "fragment requires a statement");
        }
        let mut statements = Vec::new();
        while self.statement_start()? {
            statements.push(self.statement()?);
        }
        if self.keyword("each") {
            return self.unsupported("iteration is outside this profile");
        }
        Ok(Recipe {
            path: String::new(),
            raw: false,
            statements,
        })
    }

    fn finish(&mut self) -> Parsed<()> {
        self.skip()?;
        if self.at == self.bytes.len() {
            return Ok(());
        }
        if [
            "create",
            "rename",
            "remove",
            "each",
            "line_matches",
            "frozen",
            "lines",
            "indent",
            "between",
        ]
        .iter()
        .any(|word| self.keyword(word))
        {
            return self.unsupported("Splicery /1 production is outside this profile");
        }
        self.fail(DiagnosticKind::Syntax, "trailing input")
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
        let end = start + error.error_len().unwrap_or(1).min(source.len() - start);
        diagnostic(
            DiagnosticKind::Encoding,
            start,
            end,
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
    let is_edit = parser.keyword("edit");
    let mut recipe = if is_edit {
        parser.edit()?
    } else {
        if ["create", "rename", "remove"]
            .iter()
            .any(|word| parser.keyword(word))
        {
            return parser.unsupported("file operations are outside this profile");
        }
        parser.fragment()?
    };
    parser.finish()?;
    if is_edit {
        if interactive_path.is_some_and(|path| path != recipe.path) {
            return Err(diagnostic(
                DiagnosticKind::Syntax,
                0,
                source.len(),
                "interactive_path conflicts with the edit path",
            ));
        }
    } else {
        recipe.path = interactive_path.map(str::to_owned).ok_or_else(|| {
            diagnostic(
                DiagnosticKind::Syntax,
                0,
                source.len(),
                "fragment requires interactive_path",
            )
        })?;
    }
    Ok(recipe)
}
