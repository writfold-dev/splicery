use splicery_domain::RecipeSpan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recipe {
    Edit {
        path: String,
        raw: bool,
        statements: Vec<Statement>,
    },
    Create {
        path: String,
        raw: bool,
        payload: Vec<Piece>,
    },
    Rename {
        source: String,
        destination: String,
    },
    Remove {
        path: String,
    },
}

impl Recipe {
    pub fn path(&self) -> &str {
        match self {
            Self::Edit { path, .. } | Self::Create { path, .. } | Self::Remove { path } => path,
            Self::Rename { source, .. } => source,
        }
    }

    pub fn raw(&self) -> bool {
        match self {
            Self::Edit { raw, .. } | Self::Create { raw, .. } => *raw,
            Self::Rename { .. } | Self::Remove { .. } => false,
        }
    }

    pub fn statements(&self) -> &[Statement] {
        match self {
            Self::Edit { statements, .. } => statements,
            Self::Create { .. } | Self::Rename { .. } | Self::Remove { .. } => &[],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    Binding(Binding),
    Change(Change),
    Iteration(Iteration),
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
pub struct Iteration {
    pub binder: String,
    pub collection: Collection,
    pub changes: Vec<Change>,
    pub origin: RecipeSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Collection {
    LineMatches {
        pattern: String,
        within: Option<Reference>,
        expect: usize,
        origin: RecipeSpan,
    },
    Frozen(Reference),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference {
    pub name: String,
    pub group: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Selector {
    Reference(Reference),
    OneText {
        needle: Vec<u8>,
        within: Option<Reference>,
    },
    OneLine {
        needle: Vec<u8>,
        within: Option<Reference>,
    },
    OneHex {
        needle: Vec<u8>,
        within: Option<Reference>,
    },
    Between {
        first: Reference,
        second: Reference,
    },
    /// `from @a to @b`: both anchors and everything between them.
    FromTo {
        first: Reference,
        second: Reference,
    },
    /// `whole @line`: a line binding together with its terminator.
    Whole(Reference),
    Range {
        lo: usize,
        hi: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Piece {
    Literal(Vec<u8>),
    Keep(Reference),
    LinesLike {
        reference: Reference,
        lines: Vec<Vec<u8>>,
    },
    Indent {
        prefix: Vec<u8>,
        payload: Vec<Piece>,
    },
}
