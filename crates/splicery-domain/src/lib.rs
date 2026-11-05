use sha2::{Digest, Sha256};

pub const MAX_RECIPE_BYTES: usize = 256 * 1024;
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CANDIDATE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SPLICES: usize = 4096;
pub const MAX_MATCHES: usize = 4096;
pub const MAX_REPORT_BYTES: usize = 16 * 1024;
pub const MAX_REGEX_SOURCE_BYTES: usize = 4096;
pub const MAX_REGEX_AUTOMATON_BYTES: usize = 1024 * 1024;
pub const MAX_AGGREGATE_BYTE_VISITS: usize = 128 * 1024 * 1024;
pub const MAX_NESTING_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Text,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    lo: usize,
    hi: usize,
}

impl SourceSpan {
    pub fn new(image_len: usize, lo: usize, hi: usize) -> Option<Self> {
        if lo <= hi && hi <= image_len {
            Some(Self { lo, hi })
        } else {
            None
        }
    }

    pub fn insertion(image_len: usize, at: usize) -> Option<Self> {
        Self::new(image_len, at, at)
    }

    pub fn lo(self) -> usize {
        self.lo
    }
    pub fn hi(self) -> usize {
        self.hi
    }
    pub fn width(self) -> usize {
        self.hi - self.lo
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalImage {
    bytes: Vec<u8>,
    profile: Profile,
}

impl OriginalImage {
    pub fn new(bytes: Vec<u8>, profile: Profile) -> Option<Self> {
        if bytes.len() > MAX_INPUT_BYTES {
            return None;
        }
        if profile == Profile::Text && std::str::from_utf8(&bytes).is_err() {
            return None;
        }
        Some(Self { bytes, profile })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn profile(&self) -> Profile {
        self.profile
    }
    pub fn capture_sha256(&self) -> [u8; 32] {
        let digest = Sha256::digest(&self.bytes);
        let mut output = [0u8; 32];
        output.copy_from_slice(&digest);
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Part {
    Literal(Vec<u8>),
    Copy(SourceSpan),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Splice {
    pub target: SourceSpan,
    pub replacement: Vec<Part>,
    pub origin: RecipeSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureBinding {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Unterminated,
}

impl LineEnding {
    pub fn bytes(self) -> Option<&'static [u8]> {
        match self {
            Self::Lf => Some(b"\n"),
            Self::CrLf => Some(b"\r\n"),
            Self::Unterminated => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureGroup {
    pub name: String,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCapture {
    pub span: SourceSpan,
    pub line: Option<LineEnding>,
    pub groups: Vec<CaptureGroup>,
}

impl ResolvedCapture {
    pub fn plain(span: SourceSpan) -> Self {
        Self {
            span,
            line: None,
            groups: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenSet {
    pub name: String,
    pub matcher_profile: String,
    pub capture_sha256: [u8; 32],
    pub scope: Option<SourceSpan>,
    pub members: Vec<ResolvedCapture>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilePlan {
    Create { path: String, content: Vec<u8> },
    Rename { source: String, destination: String },
    Remove { path: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Plan {
    Edit(Vec<Splice>),
    File(FilePlan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetExceeded;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Budget {
    remaining: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    pub fn new() -> Self {
        Self {
            remaining: MAX_AGGREGATE_BYTE_VISITS,
        }
    }

    pub fn charge(&mut self, amount: usize) -> Result<(), BudgetExceeded> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(BudgetExceeded)?;
        Ok(())
    }

    pub fn charge_repeated(&mut self, amount: usize, count: usize) -> Result<(), BudgetExceeded> {
        let total = amount.checked_mul(count).ok_or(BudgetExceeded)?;
        self.charge(total)
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }
}
