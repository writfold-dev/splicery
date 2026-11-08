//! Static notes about spec-defined line-body semantics that commonly surprise
//! recipe authors. A bare line target selects the line body without its
//! terminator (spec/language.md §3), and `after` inserts at the end of the
//! selection (§7). Notes never change meaning and never refuse; the CLI prints
//! them on stderr so stdout stays machine-clean.

use std::fmt;

use splicery_domain::RecipeSpan;
use splicery_syntax::ast::{Change, ChangeKind, Piece, Recipe, Selector, Statement};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryKind {
    /// `after <line> insert` whose payload is made only of `lines like` pieces:
    /// the new lines land before the target line's terminator and join it.
    LinesAfterLineBody,
    /// `delete <line>` removes the body only and leaves an empty line.
    DeleteLineBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Advisory {
    pub kind: AdvisoryKind,
    pub origin: RecipeSpan,
}

impl fmt::Display for Advisory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let RecipeSpan { start, end } = self.origin;
        match self.kind {
            AdvisoryKind::LinesAfterLineBody => write!(
                formatter,
                "note: recipe bytes [{start},{end}): `after` a line binding inserts at the end \
                 of its body, before the line terminator, so a `lines like` payload joins the \
                 two lines; insert `before` the following line, or select the line with its \
                 terminator using `one text` (spec/language.md §3, §7)"
            ),
            AdvisoryKind::DeleteLineBody => write!(
                formatter,
                "note: recipe bytes [{start},{end}): `delete` of a line binding removes its \
                 body only and leaves an empty line; select the line with its terminator using \
                 `one text` to remove the whole line (spec/language.md §3, §7)"
            ),
        }
    }
}

/// Collects notes for an edit recipe in source order. Other recipe kinds have
/// no changes and yield nothing.
pub fn line_body_advisories(recipe: &Recipe) -> Vec<Advisory> {
    let mut line_names: Vec<&str> = Vec::new();
    let mut notes = Vec::new();
    for statement in recipe.statements() {
        match statement {
            Statement::Binding(binding) => {
                if matches!(binding.selector, Selector::OneLine { .. }) {
                    line_names.push(&binding.name);
                }
            }
            Statement::Change(change) => note_change(change, &line_names, &mut notes),
            Statement::Iteration(iteration) => {
                for change in &iteration.changes {
                    note_change(change, &line_names, &mut notes);
                }
            }
        }
    }
    notes
}

fn note_change(change: &Change, line_names: &[&str], notes: &mut Vec<Advisory>) {
    let targets_line = match &change.selector {
        Selector::OneLine { .. } => true,
        Selector::Reference(reference) => {
            reference.group.is_none() && line_names.contains(&reference.name.as_str())
        }
        Selector::OneText { .. }
        | Selector::OneHex { .. }
        | Selector::Between { .. }
        | Selector::Range { .. } => false,
    };
    if !targets_line {
        return;
    }
    let line_shaped = !change.payload.is_empty()
        && change
            .payload
            .iter()
            .all(|piece| matches!(piece, Piece::LinesLike { .. }));
    let kind = match change.kind {
        ChangeKind::After if line_shaped => AdvisoryKind::LinesAfterLineBody,
        ChangeKind::Delete => AdvisoryKind::DeleteLineBody,
        ChangeKind::Replace | ChangeKind::Before | ChangeKind::After => return,
    };
    notes.push(Advisory {
        kind,
        origin: change.origin,
    });
}
