use dex_core::prelude::*;

use serde::{Deserialize, Serialize};
use utils::Reset;

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct LambdaEditor {
    active: LambdaLang,
    steel: String,
    python: String,
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub enum LambdaLang {
    Steel,
    Python,
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct LambdaArg {
    /// Must be an instance of [`crate::primitives::text::LabelEditable`]
    label: NodeUid,
    /// Must be an instance of [`crate::primitives::text::LabelEditable`]
    param_name: NodeUid,
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Lambda {
    /// Arguments taken by this [`Lambda`]
    /// Must be instances of [`LambdaArg`]
    args: Vec<NodeUid>,

    /// Must be an instance of [`LambdaEditor`]
    editor: NodeUid,
}
