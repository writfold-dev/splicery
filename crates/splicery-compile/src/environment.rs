use splicery_domain::{
    Budget, CaptureGroup, LineEnding, MAX_SPLICES, OriginalImage, Profile, RecipeSpan,
    ResolvedCapture, SourceSpan, Splice,
};

use crate::{CompileDiagnostic, CompileError};

pub(crate) const CAPTURE_ORIGIN: RecipeSpan = RecipeSpan { start: 0, end: 0 };

#[derive(Clone, Copy)]
pub(crate) struct CaptureView<'a> {
    pub(crate) span: SourceSpan,
    pub(crate) line: Option<LineEnding>,
    pub(crate) groups: &'a [CaptureGroup],
}

impl<'a> CaptureView<'a> {
    pub(crate) fn plain(span: SourceSpan) -> Self {
        Self {
            span,
            line: None,
            groups: &[],
        }
    }

    pub(crate) fn resolved(self) -> ResolvedCapture {
        ResolvedCapture {
            span: self.span,
            line: self.line,
            groups: self.groups.to_vec(),
        }
    }
}

pub(crate) struct Binding {
    pub(crate) name: String,
    pub(crate) capture: ResolvedCapture,
}

pub(crate) struct Lowering<'a> {
    pub(crate) image: &'a OriginalImage,
    pub(crate) budget: Budget,
    pub(crate) splices: Vec<Splice>,
    capture_hash: Option<[u8; 32]>,
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(image: &'a OriginalImage) -> Self {
        Self {
            image,
            budget: Budget::new(),
            splices: Vec::new(),
            capture_hash: None,
        }
    }

    pub(crate) fn charge(
        &mut self,
        amount: usize,
        origin: RecipeSpan,
    ) -> Result<(), CompileDiagnostic> {
        self.budget
            .charge(amount)
            .map_err(|_| CompileDiagnostic::new(CompileError::BudgetExceeded, origin))
    }

    pub(crate) fn charge_repeated(
        &mut self,
        amount: usize,
        count: usize,
        origin: RecipeSpan,
    ) -> Result<(), CompileDiagnostic> {
        self.budget
            .charge_repeated(amount, count)
            .map_err(|_| CompileDiagnostic::new(CompileError::BudgetExceeded, origin))
    }

    pub(crate) fn capture_sha256(
        &mut self,
        origin: RecipeSpan,
    ) -> Result<[u8; 32], CompileDiagnostic> {
        if let Some(hash) = self.capture_hash {
            return Ok(hash);
        }
        self.charge(self.image.bytes().len(), origin)?;
        let hash = self.image.capture_sha256();
        self.capture_hash = Some(hash);
        Ok(hash)
    }

    pub(crate) fn push_splice(&mut self, splice: Splice) -> Result<(), CompileDiagnostic> {
        if self.splices.len() == MAX_SPLICES {
            return Err(CompileDiagnostic::new(
                CompileError::TooManySplices,
                splice.origin,
            ));
        }
        self.splices.push(splice);
        Ok(())
    }
}

pub(crate) fn find_binding<'a>(bindings: &'a [Binding], name: &str) -> Option<&'a ResolvedCapture> {
    bindings
        .iter()
        .find(|binding| binding.name == name)
        .map(|binding| &binding.capture)
}

fn utf8_boundary(bytes: &[u8], at: usize) -> bool {
    at == 0 || at == bytes.len() || bytes.get(at).is_some_and(|byte| byte & 0xc0 != 0x80)
}

pub(crate) fn checked_span(
    lowering: &Lowering,
    span: SourceSpan,
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    let checked = SourceSpan::new(lowering.image.bytes().len(), span.lo(), span.hi())
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
    if lowering.image.profile() == Profile::Text
        && (!utf8_boundary(lowering.image.bytes(), checked.lo())
            || !utf8_boundary(lowering.image.bytes(), checked.hi()))
    {
        return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
    }
    Ok(checked)
}

pub(crate) fn validate_capture(
    capture: &ResolvedCapture,
    lowering: &mut Lowering,
    origin: RecipeSpan,
) -> Result<(), CompileDiagnostic> {
    let parent = checked_span(lowering, capture.span, origin)?;
    lowering.charge(parent.width(), origin)?;
    if let Some(line) = capture.line {
        let bytes = lowering.image.bytes();
        let body = &bytes[parent.lo()..parent.hi()];
        let starts_line = parent.lo() == 0 || bytes[parent.lo() - 1] == b'\n';
        let valid = starts_line
            && !body.contains(&b'\n')
            && match line {
                LineEnding::Unterminated => parent.hi() == bytes.len(),
                LineEnding::Lf => {
                    body.last() != Some(&b'\r') && bytes.get(parent.hi()) == Some(&b'\n')
                }
                LineEnding::CrLf => {
                    bytes.get(parent.hi()..parent.hi().saturating_add(2)) == Some(b"\r\n")
                }
            };
        if !valid {
            return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
        }
        lowering.charge(line.bytes().map_or(0, |bytes| bytes.len()), origin)?;
    }
    for (index, group) in capture.groups.iter().enumerate() {
        if capture.groups[..index]
            .iter()
            .any(|earlier| earlier.name == group.name)
        {
            return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
        }
        if let Some(span) = group.span {
            let span = checked_span(lowering, span, origin)?;
            if span.lo() < parent.lo() || span.hi() > parent.hi() {
                return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
            }
            lowering.charge(span.width(), origin)?;
        }
    }
    Ok(())
}
