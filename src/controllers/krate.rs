pub mod downloads;
pub mod follow;
pub mod metadata;
pub mod owners;
pub mod publish;
pub mod search;

use super::prelude::*;

pub(crate) fn extract_crate_name(req: &dyn RequestExt) -> String {
    crate::models::krate::Crate::decode_file_safe_name(&req.params()["crate_id"])
}
