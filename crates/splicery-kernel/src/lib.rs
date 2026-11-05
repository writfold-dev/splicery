use splicery_domain::{
    MAX_CANDIDATE_BYTES, MAX_PAYLOAD_BYTES, MAX_SPLICES, OriginalImage, Part, Profile, RecipeSpan,
    SourceSpan, Splice,
};
#[cfg(verus_keep_ghost)]
use vstd::assert_seqs_equal;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReport {
    pub target: SourceSpan,
    pub replacement: Vec<u8>,
    pub origin: RecipeSpan,
}

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
        if splices.len() > MAX_SPLICES {
            return Err(KernelError::TooManySplices);
        }
        let mut payload_bytes = 0usize;
        for splice in &splices {
            let replacement_bytes = validate_splice(&image, splice)?;
            payload_bytes = payload_bytes
                .checked_add(replacement_bytes)
                .ok_or(KernelError::PayloadTooLarge)?;
            if payload_bytes > MAX_PAYLOAD_BYTES {
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

fn replacement_len(splice: &Splice) -> Result<usize, KernelError> {
    let mut length = 0usize;
    for part in &splice.replacement {
        let part_len = match part {
            Part::Literal(bytes) => bytes.len(),
            Part::Copy(span) => span.width(),
        };
        length = length
            .checked_add(part_len)
            .ok_or(KernelError::PayloadTooLarge)?;
        if length > MAX_PAYLOAD_BYTES {
            return Err(KernelError::PayloadTooLarge);
        }
    }
    Ok(length)
}

fn validate_splice(image: &OriginalImage, splice: &Splice) -> Result<usize, KernelError> {
    validate_span(image, splice.target)?;
    for part in &splice.replacement {
        if let Part::Copy(span) = part {
            validate_span(image, *span)?;
        }
    }
    replacement_len(splice)
}

fn candidate_len(batch: &PreparedBatch) -> Result<usize, KernelError> {
    let mut length = batch.image.bytes().len();
    for splice in &batch.splices {
        let replacement_bytes = replacement_len(splice)?;
        length = length
            .checked_sub(splice.target.width())
            .and_then(|value| value.checked_add(replacement_bytes))
            .ok_or(KernelError::CandidateTooLarge)?;
    }
    if length > MAX_CANDIDATE_BYTES {
        return Err(KernelError::CandidateTooLarge);
    }
    Ok(length)
}

fn materialize(image: &OriginalImage, splice: &Splice) -> Result<Vec<u8>, KernelError> {
    let mut replacement = Vec::with_capacity(replacement_len(splice)?);
    for part in &splice.replacement {
        match part {
            Part::Literal(bytes) => replacement.extend_from_slice(bytes),
            Part::Copy(span) => {
                replacement.extend_from_slice(&image.bytes()[span.lo()..span.hi()]);
            }
        }
    }
    Ok(replacement)
}

fn emit_gaps(input: &[u8], edits: &[EditReport], output_len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(output_len);
    let mut cursor = 0usize;
    for edit in edits {
        bytes.extend_from_slice(&input[cursor..edit.target.lo()]);
        bytes.extend_from_slice(&edit.replacement);
        cursor = edit.target.hi();
    }
    bytes.extend_from_slice(&input[cursor..]);
    bytes
}

pub fn render(batch: &PreparedBatch) -> Result<Candidate, KernelError> {
    let output_len = candidate_len(batch)?;
    let mut edits = Vec::with_capacity(batch.splices.len());
    for splice in &batch.splices {
        edits.push(EditReport {
            target: splice.target,
            replacement: materialize(&batch.image, splice)?,
            origin: splice.origin,
        });
    }
    let input = batch.image.bytes();
    let bytes = if let [edit] = edits.as_slice() {
        apply_at(
            input,
            edit.target.lo(),
            edit.target.width(),
            &edit.replacement,
        )
        .ok_or(KernelError::OutOfBounds)?
    } else {
        emit_gaps(input, &edits, output_len)
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

/// Replace exactly `width` bytes at `lo`, refusing invalid cuts and oversized output.
pub fn apply_at(input: &[u8], lo: usize, width: usize, replacement: &[u8])
    -> (result: Option<Vec<u8>>)
    requires
        input@.len() <= 8 * 1024 * 1024,
        replacement@.len() <= 16 * 1024 * 1024,
    ensures
        match result {
            Some(output) => {
                lo as int <= input@.len()
                && width as int <= input@.len() - lo as int
                && output@.len() == input@.len() - width as int + replacement@.len()
                && output@.len() <= 16 * 1024 * 1024
                && output@ == input@.subrange(0, lo as int)
                    + replacement@
                    + input@.subrange(lo as int + width as int, input@.len() as int)
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
                + input@.subrange(lo as int + width as int, input@.len() as int)
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
        let target = SourceSpan::new(usize::MAX, lo, hi).ok_or(KernelError::OutOfBounds)?;
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
