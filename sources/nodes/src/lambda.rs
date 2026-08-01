use dex_core::prelude::*;

use serde::{Deserialize, Serialize};
use utils::Reset;

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Lambda {
    args: Vec<NodeUid>,
}

pub struct LambdaBody {
    steel: String,
    python: String,
}
