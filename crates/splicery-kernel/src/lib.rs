//! The Splicery edit kernel as production calls it.
//!
//! Every decision — batch validation, canonical ordering, rendering and
//! `apply_at` — is made by `splicery-splice`, whose Aeneas translation is
//! proved in `proofs/`. This facade only maps the checked domain types onto
//! the kernel's plain data and back, and adds one check the kernel does not
//! make: a text-profile candidate must be well-formed UTF-8, decided by
//! `core::str::from_utf8` (tested, not proved).
use splicery_domain::{EditTarget, OriginalImage, Part, Profile, RecipeSpan, SourceSpan, Splice};
use splicery_splice as kernel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    bytes: Vec<u8>,
    edits: Vec<EditReport>,
    no_change: bool,
}

impl Candidate {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn edits(&self) -> &[EditReport] {
        &self.edits
    }
    pub fn no_change(&self) -> bool {
        self.no_change
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelError {
    OutOfBounds,
    Overlap,
    TooManySplices,
    PayloadTooLarge,
    CandidateTooLarge,
    InvalidUtf8Boundary,
    NoChange,
}

impl From<kernel::Refusal> for KernelError {
    fn from(refusal: kernel::Refusal) -> Self {
        match refusal {
            kernel::Refusal::OutOfBounds => Self::OutOfBounds,
            kernel::Refusal::Overlap => Self::Overlap,
            kernel::Refusal::TooManySplices => Self::TooManySplices,
            kernel::Refusal::PayloadTooLarge => Self::PayloadTooLarge,
            kernel::Refusal::CandidateTooLarge => Self::CandidateTooLarge,
            kernel::Refusal::InvalidUtf8Boundary => Self::InvalidUtf8Boundary,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReport {
    pub target: EditTarget,
    pub replacement: Vec<u8>,
    pub origin: RecipeSpan,
}

/// A batch the kernel accepted: the splices in canonical order, plus the
/// kernel's own view of them (authored order) and the canonical order it
/// returned, which `render` replays.
#[derive(Clone, Debug)]
pub struct PreparedBatch {
    image: OriginalImage,
    splices: Vec<Splice>,
    edits: Vec<kernel::Edit>,
    order: Vec<usize>,
}

fn cut(capture_sha256: [u8; 32], lo: usize, hi: usize) -> kernel::Cut {
    kernel::Cut {
        capture: capture_sha256,
        lo,
        hi,
    }
}

fn target_cut(target: EditTarget) -> kernel::Cut {
    cut(target.capture_sha256(), target.lo(), target.hi())
}

fn span_cut(span: SourceSpan) -> kernel::Cut {
    cut(span.capture_sha256(), span.lo(), span.hi())
}

fn kernel_edit(splice: &Splice) -> kernel::Edit {
    kernel::Edit {
        target: target_cut(splice.target),
        parts: splice
            .replacement
            .iter()
            .map(|part| match part {
                Part::Literal(bytes) => kernel::Piece::Literal(bytes.clone()),
                Part::Copy(span) => kernel::Piece::Copy(span_cut(*span)),
            })
            .collect(),
    }
}

impl PreparedBatch {
    pub fn new(image: OriginalImage, splices: Vec<Splice>) -> Result<Self, KernelError> {
        let edits: Vec<kernel::Edit> = splices.iter().map(kernel_edit).collect();
        let order = kernel::prepare(
            image.bytes(),
            image.profile() == Profile::Text,
            &image.capture_sha256(),
            &edits,
        )?;
        // `order` is a permutation of the splice positions
        // (`prepare_order_perm`), so every slot is taken exactly once.
        let mut slots: Vec<Option<Splice>> = splices.into_iter().map(Some).collect();
        let splices = order
            .iter()
            .filter_map(|&index| slots.get_mut(index).and_then(Option::take))
            .collect();
        Ok(Self {
            image,
            splices,
            edits,
            order,
        })
    }

    pub fn image(&self) -> &OriginalImage {
        &self.image
    }
    pub fn splices(&self) -> &[Splice] {
        &self.splices
    }
}

pub fn render(batch: &PreparedBatch) -> Result<Candidate, KernelError> {
    let input = batch.image.bytes();
    let rendered = kernel::render(input, &batch.edits, &batch.order)?;
    if batch.image.profile() == Profile::Text && std::str::from_utf8(&rendered.bytes).is_err() {
        return Err(KernelError::InvalidUtf8Boundary);
    }
    let edits = batch
        .splices
        .iter()
        .zip(rendered.replacements)
        .map(|(splice, replacement)| EditReport {
            target: splice.target,
            replacement,
            origin: splice.origin,
        })
        .collect();
    Ok(Candidate {
        bytes: rendered.bytes,
        edits,
        no_change: rendered.no_change,
    })
}

/// Replace exactly `width` bytes at `lo`, refusing invalid cuts and oversized
/// output (`splicery_splice::apply_at`, proved as `apply_at_spec`).
pub fn apply_at(input: &[u8], lo: usize, width: usize, replacement: &[u8]) -> Option<Vec<u8>> {
    kernel::apply_at(input, lo, width, replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: &[u8], profile: Profile) -> Result<OriginalImage, KernelError> {
        OriginalImage::new(bytes.to_vec(), profile).ok_or(KernelError::OutOfBounds)
    }

    fn splice(
        image: &OriginalImage,
        lo: usize,
        hi: usize,
        replacement: Vec<Part>,
    ) -> Result<Splice, KernelError> {
        let target = EditTarget::new(image, lo, hi).ok_or(KernelError::OutOfBounds)?;
        let origin = RecipeSpan { start: 0, end: 1 };
        Ok(Splice {
            target,
            replacement,
            origin,
        })
    }

    #[test]
    fn renders_original_gaps_and_copies() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let copy = SourceSpan::new(&original, 4, 6).ok_or(KernelError::OutOfBounds)?;
        let change = splice(
            &original,
            1,
            3,
            vec![Part::Literal(b"XY".to_vec()), Part::Copy(copy)],
        )?;
        let candidate = render(&PreparedBatch::new(original, vec![change])?)?;
        assert_eq!(candidate.bytes(), b"aXYefdef");
        assert_eq!(candidate.edits()[0].replacement, b"XYef");
        Ok(())
    }

    #[test]
    fn rejects_insertion_at_replacement_boundaries() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let result = PreparedBatch::new(
            original.clone(),
            vec![
                splice(&original, 1, 3, vec![])?,
                splice(&original, 3, 3, vec![Part::Literal(b"x".to_vec())])?,
            ],
        )
        .map(|_| ());
        assert_eq!(result, Err(KernelError::Overlap));
        Ok(())
    }

    #[test]
    fn permits_adjacent_nonempty_replacements() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let batch = PreparedBatch::new(
            original.clone(),
            vec![
                splice(&original, 3, 4, vec![Part::Literal(b"X".to_vec())])?,
                splice(&original, 1, 3, vec![Part::Literal(b"Y".to_vec())])?,
            ],
        )?;
        assert_eq!(render(&batch)?.bytes(), b"aYXef");
        Ok(())
    }

    #[test]
    fn rejects_invalid_text_candidate_without_result() -> Result<(), KernelError> {
        let original = image("aé".as_bytes(), Profile::Text)?;
        let change = splice(&original, 0, 1, vec![Part::Literal(vec![0xff])])?;
        let batch = PreparedBatch::new(original, vec![change])?;
        assert_eq!(render(&batch), Err(KernelError::InvalidUtf8Boundary));
        Ok(())
    }

    #[test]
    fn identical_candidate_is_no_change() -> Result<(), KernelError> {
        let original = image(b"abc", Profile::Raw)?;
        let copy = SourceSpan::new(&original, 1, 2).ok_or(KernelError::OutOfBounds)?;
        let change = splice(&original, 1, 2, vec![Part::Copy(copy)])?;
        let candidate = render(&PreparedBatch::new(original, vec![change])?)?;
        assert!(candidate.no_change());
        assert_eq!(candidate.bytes(), b"abc");
        Ok(())
    }

    #[test]
    fn rejects_foreign_targets_and_copies_despite_matching_lengths() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let foreign = image(b"uvwxyz", Profile::Raw)?;
        assert_eq!(original.bytes().len(), foreign.bytes().len());
        assert_ne!(original.capture_sha256(), foreign.capture_sha256());

        let valid_copy = SourceSpan::new(&original, 4, 6).ok_or(KernelError::OutOfBounds)?;
        let valid = splice(&original, 1, 3, vec![Part::Copy(valid_copy)])?;
        assert!(PreparedBatch::new(original.clone(), vec![valid]).is_ok());

        let foreign_target = EditTarget::new(&foreign, 1, 3).ok_or(KernelError::OutOfBounds)?;
        let change = Splice {
            target: foreign_target,
            replacement: vec![Part::Copy(valid_copy)],
            origin: RecipeSpan { start: 0, end: 1 },
        };
        assert_eq!(
            PreparedBatch::new(original.clone(), vec![change]).map(|_| ()),
            Err(KernelError::OutOfBounds)
        );

        let foreign_copy = SourceSpan::new(&foreign, 4, 6).ok_or(KernelError::OutOfBounds)?;
        let change = splice(&original, 1, 3, vec![Part::Copy(foreign_copy)])?;
        assert_eq!(
            PreparedBatch::new(original, vec![change]).map(|_| ()),
            Err(KernelError::OutOfBounds)
        );
        Ok(())
    }

    #[test]
    fn kernel_limits_match_domain() {
        assert_eq!(splicery_splice::MAX_SPLICES, splicery_domain::MAX_SPLICES);
        assert_eq!(
            splicery_splice::MAX_PAYLOAD_BYTES,
            splicery_domain::MAX_PAYLOAD_BYTES
        );
        assert_eq!(
            splicery_splice::MAX_CANDIDATE_BYTES,
            splicery_domain::MAX_CANDIDATE_BYTES
        );
    }

    #[test]
    fn splices_and_reports_follow_canonical_order() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let batch = PreparedBatch::new(
            original.clone(),
            vec![
                splice(&original, 4, 5, vec![Part::Literal(b"Y".to_vec())])?,
                splice(&original, 0, 1, vec![Part::Literal(b"X".to_vec())])?,
            ],
        )?;
        assert_eq!(batch.splices()[0].target.lo(), 0);
        assert_eq!(batch.splices()[1].target.lo(), 4);
        let candidate = render(&batch)?;
        assert_eq!(candidate.bytes(), b"XbcdYf");
        assert_eq!(candidate.edits()[0].replacement, b"X");
        assert_eq!(candidate.edits()[1].replacement, b"Y");
        assert_eq!(candidate.edits()[1].target.lo(), 4);
        Ok(())
    }

    #[test]
    fn empty_batch_is_identity() -> Result<(), KernelError> {
        let original = image(b"abc", Profile::Text)?;
        let candidate = render(&PreparedBatch::new(original, Vec::new())?)?;
        assert!(candidate.no_change());
        assert_eq!(candidate.bytes(), b"abc");
        assert!(candidate.edits().is_empty());
        Ok(())
    }

    #[test]
    fn rejects_text_cut_inside_a_character() -> Result<(), KernelError> {
        let original = image("aé".as_bytes(), Profile::Text)?;
        let inside = splice(&original, 2, 3, Vec::new())?;
        assert_eq!(
            PreparedBatch::new(original.clone(), vec![inside]).map(|_| ()),
            Err(KernelError::InvalidUtf8Boundary)
        );
        let copy = SourceSpan::new(&original, 1, 2).ok_or(KernelError::OutOfBounds)?;
        let copies_half = splice(&original, 0, 1, vec![Part::Copy(copy)])?;
        assert_eq!(
            PreparedBatch::new(original, vec![copies_half]).map(|_| ()),
            Err(KernelError::InvalidUtf8Boundary)
        );
        Ok(())
    }

    #[test]
    fn refuses_more_than_max_splices_before_anything_else() -> Result<(), KernelError> {
        let original = image(b"abc", Profile::Raw)?;
        let one = splice(&original, 0, 0, Vec::new())?;
        let many = vec![one; splicery_domain::MAX_SPLICES + 1];
        assert_eq!(
            PreparedBatch::new(original, many).map(|_| ()),
            Err(KernelError::TooManySplices)
        );
        Ok(())
    }

    #[test]
    fn rejects_overlap_in_any_authored_order() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let first = splice(&original, 1, 4, Vec::new())?;
        let second = splice(&original, 3, 5, Vec::new())?;
        for batch in [vec![first.clone(), second.clone()], vec![second, first]] {
            assert_eq!(
                PreparedBatch::new(original.clone(), batch).map(|_| ()),
                Err(KernelError::Overlap)
            );
        }
        Ok(())
    }

    #[test]
    fn apply_at_checks_cut_and_copies_exactly() {
        assert_eq!(
            apply_at(b"abcdef", 2, 2, b"XY").as_deref(),
            Some(b"abXYef".as_slice())
        );
        assert_eq!(apply_at(b"abc", 2, 2, b""), None);
        assert_eq!(apply_at(b"abc", 4, 0, b""), None);
    }
}
