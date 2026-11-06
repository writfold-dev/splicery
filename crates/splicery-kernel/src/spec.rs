use crate::{EditReport, KernelError};
#[cfg(verus_keep_ghost)]
use splicery_domain::EditTarget;
#[cfg(verus_keep_ghost)]
use splicery_domain::RecipeSpan;
#[cfg(verus_keep_ghost)]
use splicery_domain::SourceSpan;
use splicery_domain::{Part, Splice};
#[cfg(verus_keep_ghost)]
use vstd::assert_seqs_equal;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

vstd::prelude::verus! {

#[cfg(verus_keep_ghost)]
#[verifier::external_type_specification] #[verifier::external_body]
pub struct ExSourceSpan(SourceSpan);
#[cfg(verus_keep_ghost)]
#[verifier::external_type_specification] #[verifier::external_body]
pub struct ExEditTarget(EditTarget);
#[cfg(verus_keep_ghost)]
#[verifier::external_type_specification] pub struct ExPart(Part);
#[cfg(verus_keep_ghost)]
#[verifier::external_type_specification] pub struct ExSplice(Splice);
#[cfg(verus_keep_ghost)]
#[verifier::external_type_specification] pub struct ExRecipeSpan(RecipeSpan);

pub uninterp spec fn span_lo(span: SourceSpan) -> int;
pub uninterp spec fn span_hi(span: SourceSpan) -> int;

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn source_span_lo(span: SourceSpan) -> (result: usize)
    ensures result as int == span_lo(span),
{ span.lo() }

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn source_span_hi(span: SourceSpan) -> (result: usize)
    ensures result as int == span_hi(span),
{ span.hi() }

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn source_span_width(span: SourceSpan) -> (result: usize)
    requires span_lo(span) <= span_hi(span),
    ensures result as int == span_hi(span) - span_lo(span),
{ span.width() }

pub uninterp spec fn target_lo(target: EditTarget) -> int;
pub uninterp spec fn target_hi(target: EditTarget) -> int;

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn edit_target_lo(target: EditTarget) -> (result: usize)
    ensures result as int == target_lo(target),
{ target.lo() }

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn edit_target_hi(target: EditTarget) -> (result: usize)
    ensures result as int == target_hi(target),
{ target.hi() }

#[cfg(verus_keep_ghost)]
#[verifier::external_fn_specification]
pub fn edit_target_width(target: EditTarget) -> (result: usize)
    requires target_lo(target) <= target_hi(target),
    ensures result as int == target_hi(target) - target_lo(target),
{ target.width() }

pub open spec fn valid_target(input_len: int, target: EditTarget) -> bool {
    0 <= target_lo(target) <= target_hi(target) <= input_len
}

pub open spec fn seq_len<T>(items: Seq<T>) -> int {
    items.len() as int
}

pub open spec fn valid_span(input_len: int, span: SourceSpan) -> bool {
    0 <= span_lo(span) <= span_hi(span) <= input_len
}

pub open spec fn part_valid(input_len: int, part: Part) -> bool {
    match part {
        Part::Literal(_) => true,
        Part::Copy(span) => valid_span(input_len, span),
    }
}

pub open spec fn valid_parts(input_len: int, parts: Seq<Part>) -> bool {
    forall|i: int| 0 <= i < seq_len(parts) ==> #[trigger] part_valid(input_len, parts[i])
}

pub open spec fn part_len(part: Part) -> int {
    match part {
        Part::Literal(bytes) => seq_len(bytes@),
        Part::Copy(span) => span_hi(span) - span_lo(span),
    }
}

pub open spec fn parts_size(parts: Seq<Part>, i: nat) -> int
    recommends i <= parts.len(),
    decreases i,
{
    if i == 0 { 0 } else { parts_size(parts, (i - 1) as nat) + part_len(parts[i - 1]) }
}

pub open spec fn part_bytes(input: Seq<u8>, part: Part) -> Seq<u8>
    recommends match part {
        Part::Literal(_) => true,
        Part::Copy(span) => valid_span(seq_len(input), span),
    }
{
    match part {
        Part::Literal(bytes) => bytes@,
        Part::Copy(span) => input.subrange(span_lo(span), span_hi(span)),
    }
}

pub open spec fn parts_bytes(input: Seq<u8>, parts: Seq<Part>, i: nat) -> Seq<u8>
    recommends i <= parts.len(), valid_parts(seq_len(input), parts),
    decreases i,
{
    if i == 0 { Seq::empty() } else {
        parts_bytes(input, parts, (i - 1) as nat) + part_bytes(input, parts[i - 1])
    }
}

pub open spec fn normalized(input_len: int, splices: Seq<Splice>) -> bool {
    &&& forall|i: int| 0 <= i < seq_len(splices) ==> (
        valid_target(input_len, splices[i].target)
        && valid_parts(input_len, splices[i].replacement@)
        && parts_size(splices[i].replacement@, splices[i].replacement@.len()) <= 16 * 1024 * 1024
    )
    &&& forall|i: int| 0 <= i < seq_len(splices) - 1 ==>
        #[trigger] target_hi(splices[i].target) <= target_lo(splices[i + 1].target)
}

pub open spec fn batch_delta(splices: Seq<Splice>, i: nat) -> int
    recommends i <= splices.len(),
    decreases i,
{
    if i == 0 { 0 } else {
        let splice = splices[i - 1];
        batch_delta(splices, (i - 1) as nat) - (target_hi(splice.target) - target_lo(splice.target))
            + parts_size(splice.replacement@, splice.replacement@.len())
    }
}

pub open spec fn emitted(input: Seq<u8>, splices: Seq<Splice>, i: nat) -> Seq<u8>
    recommends i <= splices.len(), normalized(seq_len(input), splices),
    decreases i,
{
    if i == 0 { Seq::empty() } else {
        let splice = splices[i - 1];
        let cursor = if i == 1 { 0 } else { target_hi(splices[i - 2].target) };
        emitted(input, splices, (i - 1) as nat)
            + input.subrange(cursor, target_lo(splice.target))
            + parts_bytes(input, splice.replacement@, splice.replacement@.len())
    }
}

pub open spec fn batch_output(input: Seq<u8>, splices: Seq<Splice>) -> Seq<u8>
    recommends normalized(seq_len(input), splices),
{
    let cursor = if seq_len(splices) == 0 { 0 } else { target_hi(splices.last().target) };
    emitted(input, splices, splices.len()) + input.subrange(cursor, seq_len(input))
}

pub(crate) fn replacement_len(_input: &[u8], splice: &Splice) -> (result: Result<usize, KernelError>)
    requires valid_parts(seq_len(_input@), splice.replacement@),
    ensures match result {
        Ok(length) => length <= 16 * 1024 * 1024
            && length as int == parts_size(splice.replacement@, splice.replacement@.len()),
        Err(_) => true,
    },
{
    let parts = &splice.replacement;
    let mut length = 0usize;
    let mut i = 0usize;
    while i < parts.len()
        invariant
            i <= parts.len(),
            valid_parts(seq_len(_input@), splice.replacement@),
            length <= 16 * 1024 * 1024,
            length as int == parts_size(parts@, i as nat),
        decreases parts.len() - i,
    {
        let amount = match &parts[i] {
            Part::Literal(bytes) => bytes.len(),
            Part::Copy(span) => span.hi().checked_sub(span.lo()).ok_or(KernelError::OutOfBounds)?,
        };
        let next = match length.checked_add(amount) {
            Some(value) => value,
            None => return Err(KernelError::PayloadTooLarge),
        };
        if next > 16 * 1024 * 1024 { return Err(KernelError::PayloadTooLarge); }
        length = next;
        i += 1;
    }
    Ok(length)
}

pub(crate) fn candidate_len(input: &[u8], splices: &[Splice]) -> (result: Result<usize, KernelError>)
    requires seq_len(input@) <= 8 * 1024 * 1024, normalized(seq_len(input@), splices@),
    ensures match result {
        Ok(length) => length <= 16 * 1024 * 1024
            && length as int == seq_len(input@) + batch_delta(splices@, splices@.len()),
        Err(_) => true,
    },
{
    let mut length = input.len();
    let mut i = 0usize;
    while i < splices.len()
        invariant
            i <= splices.len(),
            normalized(seq_len(input@), splices@),
            length as int == seq_len(input@) + batch_delta(splices@, i as nat),
        decreases splices.len() - i,
    {
        let splice = &splices[i];
        let replacement = replacement_len(input, splice)?;
        let retained = match length.checked_sub(splice.target.width()) {
            Some(value) => value,
            None => return Err(KernelError::CandidateTooLarge),
        };
        length = match retained.checked_add(replacement) {
            Some(value) => value,
            None => return Err(KernelError::CandidateTooLarge),
        };
        i += 1;
    }
    if length > 16 * 1024 * 1024 { return Err(KernelError::CandidateTooLarge); }
    Ok(length)
}

pub(crate) fn materialize(input: &[u8], splice: &Splice) -> (result: Result<Vec<u8>, KernelError>)
    requires
        valid_parts(seq_len(input@), splice.replacement@),
        parts_size(splice.replacement@, splice.replacement@.len()) <= 16 * 1024 * 1024,
    ensures match result {
        Ok(bytes) => {
            &&& bytes@ == parts_bytes(input@, splice.replacement@, splice.replacement@.len())
            &&& seq_len(bytes@) == parts_size(splice.replacement@, splice.replacement@.len())
        },
        Err(_) => true,
    },
{
    let parts = &splice.replacement;
    let mut replacement = Vec::with_capacity(replacement_len(input, splice)?);
    let mut i = 0usize;
    while i < parts.len()
        invariant
            i <= parts.len(),
            valid_parts(seq_len(input@), parts@),
            replacement@ == parts_bytes(input@, parts@, i as nat),
            seq_len(replacement@) == parts_size(parts@, i as nat),
        decreases parts.len() - i,
    {
        match &parts[i] {
            Part::Literal(bytes) => replacement.extend_from_slice(bytes),
            Part::Copy(span) => {
                replacement.extend_from_slice(input.get(span.lo()..span.hi()).ok_or(KernelError::OutOfBounds)?);
            }
        }
        i += 1;
        proof {
            assert_seqs_equal!(replacement@ == parts_bytes(input@, parts@, i as nat));
            assert(seq_len(replacement@) == parts_size(parts@, i as nat));
        }
    }
    Ok(replacement)
}

pub(crate) fn emit_gaps(input: &[u8], splices: &[Splice], output_len: usize)
    -> (result: Result<(Vec<u8>, Vec<EditReport>), KernelError>)
    requires
        seq_len(input@) <= 8 * 1024 * 1024,
        normalized(seq_len(input@), splices@),
        output_len as int == seq_len(input@) + batch_delta(splices@, splices@.len()),
        output_len <= 16 * 1024 * 1024,
    ensures match result {
        Ok((bytes, edits)) => {
            &&& bytes@ == batch_output(input@, splices@)
            &&& bytes.len() == output_len
            &&& seq_len(edits@) == seq_len(splices@)
            &&& forall|i: int| 0 <= i < seq_len(splices@) ==> {
                &&& #[trigger] edits@[i as int].target == splices@[i].target
                &&& edits@[i as int].origin == splices@[i].origin
                &&& edits@[i as int].replacement@ == parts_bytes(
                    input@, splices@[i].replacement@, splices@[i].replacement@.len())
            }
        },
        Err(_) => true,
    },
{
    let mut bytes = Vec::with_capacity(output_len);
    let mut edits: Vec<EditReport> = Vec::with_capacity(splices.len());
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < splices.len()
        invariant
            i <= splices.len(),
            normalized(seq_len(input@), splices@),
            cursor as int == if i == 0 { 0 } else { target_hi(splices@[i - 1].target) },
            bytes@ == emitted(input@, splices@, i as nat),
            seq_len(bytes@) == cursor as int + batch_delta(splices@, i as nat),
            seq_len(edits@) == i as int,
            forall|j: int| 0 <= j < i as int ==> {
                &&& #[trigger] edits@[j].target == splices@[j].target
                &&& edits@[j].origin == splices@[j].origin
                &&& edits@[j].replacement@ == parts_bytes(
                    input@, splices@[j].replacement@, splices@[j].replacement@.len())
            },
        decreases splices.len() - i,
    {
        let splice = &splices[i];
        let replacement = materialize(input, splice)?;
        bytes.extend_from_slice(&input[cursor..splice.target.lo()]);
        bytes.extend_from_slice(&replacement);
        edits.push(EditReport { target: splice.target, replacement, origin: splice.origin });
        cursor = splice.target.hi();
        i += 1;
        proof {
            assert(edits@.last().target == splice.target);
            assert(edits@.last().origin == splice.origin);
            assert_seqs_equal!(edits@.last().replacement@ ==
                parts_bytes(input@, splice.replacement@, splice.replacement@.len()));
            assert_seqs_equal!(bytes@ == emitted(input@, splices@, i as nat));
            assert(forall|j: int| 0 <= j < i as int ==> {
                &&& #[trigger] edits@[j].target == splices@[j].target
                &&& edits@[j].origin == splices@[j].origin
                &&& edits@[j].replacement@ == parts_bytes(
                    input@, splices@[j].replacement@, splices@[j].replacement@.len())
            });
        }
    }
    bytes.extend_from_slice(&input[cursor..]);
    proof {
        assert_seqs_equal!(bytes@ == batch_output(input@, splices@));
        assert(bytes.len() == output_len);
    }
    Ok((bytes, edits))
}

}
