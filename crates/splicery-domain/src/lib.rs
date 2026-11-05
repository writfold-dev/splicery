pub const MAX_RECIPE_BYTES: usize = 256 * 1024;
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CANDIDATE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SPLICES: usize = 4096;
pub const MAX_MATCHES: usize = 4096;
pub const MAX_REPORT_BYTES: usize = 16 * 1024;

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
