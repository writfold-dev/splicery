use splicery_domain::RecipeSpan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub path: String,
    pub raw: bool,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    Binding(Binding),
    Change(Change),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    pub name: String,
    pub selector: Selector,
    pub origin: RecipeSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Replace,
    Delete,
    Before,
    After,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub selector: Selector,
    pub payload: Vec<Piece>,
    pub origin: RecipeSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Selector {
    Reference(String),
    OneText {
        needle: Vec<u8>,
        within: Option<String>,
    },
    OneHex {
        needle: Vec<u8>,
        within: Option<String>,
    },
    Range {
        lo: usize,
        hi: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Piece {
    Literal(Vec<u8>),
    Keep(String),
}
