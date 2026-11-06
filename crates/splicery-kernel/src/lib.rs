use splicery_domain::{EditTarget, OriginalImage, Part, Profile, RecipeSpan, SourceSpan, Splice};
#[cfg(verus_keep_ghost)]
use vstd::assert_seqs_equal;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

mod spec;
use spec::{candidate_len, emit_gaps, materialize};

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

#[derive(Clone, Debug)]
pub struct PreparedBatch {
    image: OriginalImage,
    splices: Vec<Splice>,
}

impl PreparedBatch {
    pub fn new(image: OriginalImage, mut splices: Vec<Splice>) -> Result<Self, KernelError> {
        if splices.len() > 4096 {
            return Err(KernelError::TooManySplices);
        }
        let mut payload_bytes = 0usize;
        for splice in &splices {
            let replacement_bytes = validate_splice(&image, splice)?;
            payload_bytes = payload_bytes
                .checked_add(replacement_bytes)
                .ok_or(KernelError::PayloadTooLarge)?;
            if payload_bytes > 16 * 1024 * 1024 {
                return Err(KernelError::PayloadTooLarge);
            }
        }
        splices.sort_by_key(|splice| splice.target.lo());
        for pair in splices.windows(2) {
            let left = &pair[0].target;
            let right = &pair[1].target;
            if left.hi() > right.lo()
                || (left.hi() == right.lo() && (left.width() == 0 || right.width() == 0))
            {
                return Err(KernelError::Overlap);
            }
        }
        Ok(Self { image, splices })
    }

    pub fn image(&self) -> &OriginalImage {
        &self.image
    }
    pub fn splices(&self) -> &[Splice] {
        &self.splices
    }
}

fn validate_span(image: &OriginalImage, span: SourceSpan) -> Result<(), KernelError> {
    let bytes = image.bytes();
    if span.lo() > span.hi() || span.hi() > bytes.len() {
        return Err(KernelError::OutOfBounds);
    }
    if image.profile() == Profile::Text {
        let text = std::str::from_utf8(bytes).map_err(|_| KernelError::InvalidUtf8Boundary)?;
        if !text.is_char_boundary(span.lo()) || !text.is_char_boundary(span.hi()) {
            return Err(KernelError::InvalidUtf8Boundary);
        }
    }
    Ok(())
}

fn validate_target(image: &OriginalImage, target: EditTarget) -> Result<(), KernelError> {
    let bytes = image.bytes();
    if target.lo() > target.hi() || target.hi() > bytes.len() {
        return Err(KernelError::OutOfBounds);
    }
    if image.profile() == Profile::Text {
        let text = std::str::from_utf8(bytes).map_err(|_| KernelError::InvalidUtf8Boundary)?;
        if !text.is_char_boundary(target.lo()) || !text.is_char_boundary(target.hi()) {
            return Err(KernelError::InvalidUtf8Boundary);
        }
    }
    Ok(())
}

fn validate_splice(image: &OriginalImage, splice: &Splice) -> Result<usize, KernelError> {
    validate_target(image, splice.target)?;
    for part in &splice.replacement {
        if let Part::Copy(span) = part {
            validate_span(image, *span)?;
        }
    }
    spec::replacement_len(image.bytes(), splice)
}

pub fn render(batch: &PreparedBatch) -> Result<Candidate, KernelError> {
    let input = batch.image.bytes();
    let output_len = candidate_len(input, &batch.splices)?;
    let (bytes, edits) = if let [splice] = batch.splices.as_slice() {
        let replacement = materialize(input, splice)?;
        let bytes = apply_at(
            input,
            splice.target.lo(),
            splice.target.width(),
            &replacement,
        )
        .ok_or(KernelError::OutOfBounds)?;
        let edits = vec![EditReport {
            target: splice.target,
            replacement,
            origin: splice.origin,
        }];
        (bytes, edits)
    } else {
        emit_gaps(input, &batch.splices, output_len)?
    };
    if batch.image.profile() == Profile::Text && std::str::from_utf8(&bytes).is_err() {
        return Err(KernelError::InvalidUtf8Boundary);
    }
    let no_change = bytes == input;
    Ok(Candidate {
        bytes,
        edits,
        no_change,
    })
}

vstd::prelude::verus! {

pub open spec fn seq_len<T>(items: Seq<T>) -> int {
    items.len() as int
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelError {
    OutOfBounds, Overlap, TooManySplices, PayloadTooLarge, CandidateTooLarge,
    InvalidUtf8Boundary, NoChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReport {
    pub target: EditTarget,
    pub replacement: Vec<u8>,
    pub origin: RecipeSpan,
}

/// Replace exactly `width` bytes at `lo`, refusing invalid cuts and oversized output.
pub fn apply_at(input: &[u8], lo: usize, width: usize, replacement: &[u8])
    -> (result: Option<Vec<u8>>)
    requires
        seq_len(input@) <= 8 * 1024 * 1024,
        seq_len(replacement@) <= 16 * 1024 * 1024,
    ensures
        match result {
            Some(output) => {
                lo as int <= seq_len(input@)
                && width as int <= seq_len(input@) - lo as int
                && seq_len(output@) == seq_len(input@) - width as int + seq_len(replacement@)
                && seq_len(output@) <= 16 * 1024 * 1024
                && output@ == input@.subrange(0, lo as int)
                    + replacement@
                    + input@.subrange(lo as int + width as int, seq_len(input@) as int)
            },
            None => true,
        },
{
    if lo > input.len() { return None; }
    let remaining = input.len() - lo;
    if width > remaining { return None; }
    let suffix_start = lo + width;
    let retained = input.len() - width;
    let output_len = retained.checked_add(replacement.len())?;
    if output_len > 16 * 1024 * 1024 { return None; }
    let mut output = Vec::with_capacity(output_len);
    output.extend_from_slice(&input[..lo]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&input[suffix_start..]);
    proof {
        assert_seqs_equal!(
            output@ == input@.subrange(0, lo as int)
                + replacement@
                + input@.subrange(lo as int + width as int, seq_len(input@) as int)
        );
    }
    Some(output)
}

}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: &[u8], profile: Profile) -> Result<OriginalImage, KernelError> {
        OriginalImage::new(bytes.to_vec(), profile).ok_or(KernelError::OutOfBounds)
    }

    fn splice(lo: usize, hi: usize, replacement: Vec<Part>) -> Result<Splice, KernelError> {
        let target = EditTarget::new(usize::MAX, lo, hi).ok_or(KernelError::OutOfBounds)?;
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
        let copy = SourceSpan::new(6, 4, 6).ok_or(KernelError::OutOfBounds)?;
        let change = splice(1, 3, vec![Part::Literal(b"XY".to_vec()), Part::Copy(copy)])?;
        let candidate = render(&PreparedBatch::new(original, vec![change])?)?;
        assert_eq!(candidate.bytes(), b"aXYefdef");
        assert_eq!(candidate.edits()[0].replacement, b"XYef");
        Ok(())
    }

    #[test]
    fn rejects_insertion_at_replacement_boundaries() -> Result<(), KernelError> {
        let original = image(b"abcdef", Profile::Raw)?;
        let result = PreparedBatch::new(
            original,
            vec![
                splice(1, 3, vec![])?,
                splice(3, 3, vec![Part::Literal(b"x".to_vec())])?,
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
            original,
            vec![
                splice(3, 4, vec![Part::Literal(b"X".to_vec())])?,
                splice(1, 3, vec![Part::Literal(b"Y".to_vec())])?,
            ],
        )?;
        assert_eq!(render(&batch)?.bytes(), b"aYXef");
        Ok(())
    }

    #[test]
    fn rejects_invalid_text_candidate_without_result() -> Result<(), KernelError> {
        let original = image("aé".as_bytes(), Profile::Text)?;
        let change = splice(0, 1, vec![Part::Literal(vec![0xff])])?;
        let batch = PreparedBatch::new(original, vec![change])?;
        assert_eq!(render(&batch), Err(KernelError::InvalidUtf8Boundary));
        Ok(())
    }

    #[test]
    fn identical_candidate_is_no_change() -> Result<(), KernelError> {
        let original = image(b"abc", Profile::Raw)?;
        let copy = SourceSpan::new(3, 1, 2).ok_or(KernelError::OutOfBounds)?;
        let change = splice(1, 2, vec![Part::Copy(copy)])?;
        let candidate = render(&PreparedBatch::new(original, vec![change])?)?;
        assert!(candidate.no_change());
        assert_eq!(candidate.bytes(), b"abc");
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
