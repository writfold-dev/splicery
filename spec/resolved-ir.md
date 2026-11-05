# Resolved byte IR /1

This is the proposed *internal interchange* between lowering and the checked kernel. It has one atom for updating a regular file. Preserve the distinction between three objects: a source recipe (possibly containing aliases), its resolved IR over one exact image, and a host-authorized prepared edit. A serializable IR can be replayed offline over its declared image. It cannot authorize IO.

~~~text
Batch(input: ExactImageRef, profile: EditProfile, edits: Sequence<Splice>)
Splice(target: OriginalRange, parts: Sequence<Part>, origin: RecipeSpan)
Part = Literal(exactBytes) | Copy(originalRange)
OriginalRange = [SourceByteOffset, SourceByteOffset)
~~~

ExactImageRef is realm/run/worktree/file/binding/profile qualified and points to retained original bytes. Host-local aliases are expanded before admission. A copy is restricted to the same image in /1. Preserve a separate origin map recording which literal, selection, match group, or constructor emitted each part; an origin explains execution and cannot confer a write grant. Empty lists have explicit wire values. Mutable byte buffers and stringified editor commands cannot be passed into the kernel. Operations on file-set membership bypass this IR and use the respective qualified Writfold effect kinds.

## Lowering rules

| Surface | Primitive / obligation |
|---|---|
| replace S with P | splice(range(S), lower(P)) |
| delete S | splice(range(S), []) |
| before S insert P / after S insert P | splice([start,start), lower(P)) / splice([end,end), lower(P)) |
| keep(S) | Copy(checked source range for S in this image); identical preimage and allowed read |
| list of pieces | concatenate piece sequences in source order; literal bytes retain exact origination |
| each bound match | expand against one complete frozen domain; validate each body and all conflicts before returning the one batch |
| between whole lines | prove both line identities are from this capture; start at first.fullEnd and finish at second.start |
| lines like / indent | lower under pinned line profile to literals and/or original ranges; prove concatenated bytes equal direct meaning and preserve terminators |
| create / rename / remove | separate host operation with expected absence/existence; never encode as an empty file splice |

An implementation can use a piece table, scatter/gather buffers or streaming output if the same checked subject and bytes result. Enforce aggregate limits during capture expansion, normalization and materialization. Reject foreign original offsets, reversed or out-of-bounds ranges, wrong profile and intersecting destinations. The canonical IR sorts original destinations only after validating the complete authored batch; do not let sorting select a winner. Preserve diagnostic source order separately. Agree normalized digests for both clients using independent vectors; checks must distinguish structural identity from equal output (different edits can produce equal bytes).

**Worked lowering.** In the committed 01-await fixture the byte input is `let reply = transport.send(request)?;\r\n`. The call is [12,35). The resolved splice is [35,35) with Literal of hexadecimal 2e6177616974. Inserting at 36 would place the suffix after the question mark. A fixture below binds the exact source and expected outputs, avoiding ambiguous visually rendered CRLF or Unicode characters.

See [IR vectors](../qualification/ir-vectors.json). These use readable integers and hex as test notation. They are not an alternative canonical wire codec or a Writfold release schema.
