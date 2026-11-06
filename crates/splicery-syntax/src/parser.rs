mod selector;

use crate::ast::{Binding, Change, ChangeKind, Iteration, Recipe, Statement};
use crate::{Diagnostic, DiagnosticKind, Parsed, Parser, diagnostic};
use splicery_domain::{MAX_RECIPE_BYTES, RecipeSpan};

impl Parser<'_> {
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
