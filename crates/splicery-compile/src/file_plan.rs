use splicery_domain::{FilePlan, MAX_PAYLOAD_BYTES, OriginalImage, Plan};
use splicery_syntax::ast::Piece;

use crate::environment::{CAPTURE_ORIGIN, Lowering};
use crate::{CompileDiagnostic, CompileError};

pub(crate) fn lower_create(
    path: &str,
    raw: bool,
    payload: &[Piece],
    image: &OriginalImage,
) -> Result<Plan, CompileDiagnostic> {
    let mut lowering = Lowering::new(image);
    let mut length = 0usize;
    for piece in payload {
        let Piece::Literal(bytes) = piece else {
            return Err(CompileDiagnostic::new(
                CompileError::Unsupported,
                CAPTURE_ORIGIN,
            ));
        };
        length = length
            .checked_add(bytes.len())
            .filter(|length| *length <= MAX_PAYLOAD_BYTES)
            .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, CAPTURE_ORIGIN))?;
        lowering.charge(bytes.len(), CAPTURE_ORIGIN)?;
    }
    let mut content = Vec::with_capacity(length);
    for piece in payload {
        if let Piece::Literal(bytes) = piece {
            content.extend_from_slice(bytes);
        }
    }
    if !raw && std::str::from_utf8(&content).is_err() {
        return Err(CompileDiagnostic::new(
            CompileError::WrongProfile,
            CAPTURE_ORIGIN,
        ));
    }
    Ok(Plan::File(FilePlan::Create {
        path: path.to_owned(),
        content,
    }))
}
