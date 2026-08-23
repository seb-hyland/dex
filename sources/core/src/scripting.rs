use std::ffi::{CString, NulError};

use crate::{NodeUid, WorkspaceActionHandle};

/// A fully type-erased id, as seen by scripts. Replaces [`NodeUid`]
#[utils::dynamic_type(name = "NodeUid")]
#[derive(Clone, Copy)]
pub struct NodeHandle(pub NodeUid);

/// A scripting language that dex binds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptLang {
    Python,
    Steel,
}

/// A failure while running a script.
#[derive(Debug)]
pub enum ScriptError {
    /// The source contained an interior NUL byte (Python only).
    InteriorNul,
    Python(String),
    Steel(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InteriorNul => write!(f, "script source contains an interior NUL byte"),
            Self::Python(e) => write!(f, "python error: {e}"),
            Self::Steel(e) => write!(f, "steel error: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<NulError> for ScriptError {
    fn from(_: NulError) -> Self {
        Self::InteriorNul
    }
}

/// Run `source` in `lang` with `handle` as context.
pub fn run_script(
    lang: ScriptLang,
    source: &str,
    handle: &WorkspaceActionHandle,
) -> Result<(), ScriptError> {
    match lang {
        ScriptLang::Python => run_python(source, handle),
        ScriptLang::Steel => run_steel(source, handle),
    }
}

fn run_python(source: &str, handle: &WorkspaceActionHandle) -> Result<(), ScriptError> {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    let code = CString::new(source)?;
    Python::attach(|py| {
        let map_err = |e: PyErr| ScriptError::Python(e.to_string());
        let dex_mod = dex_dynamic::build_python_module(py).map_err(map_err)?;

        // Seed `dex.ws`.
        let ws = Bound::new(py, handle.clone()).map_err(map_err)?;
        dex_mod.add("ws", ws).map_err(map_err)?;

        let globals = PyDict::new(py);
        globals.set_item("dex", &dex_mod).map_err(map_err)?;

        py.run(code.as_c_str(), Some(&globals), None)
            .map_err(map_err)?;
        Ok(())
    })
}

fn run_steel(source: &str, handle: &WorkspaceActionHandle) -> Result<(), ScriptError> {
    let mut engine = dex_dynamic::build_steel_engine();

    // Seed the `ws` global.
    engine
        .register_external_value("ws", handle.clone())
        .map_err(|e| ScriptError::Steel(e.to_string()))?;

    engine
        .run(source.to_owned())
        .map(|_| ())
        .map_err(|e| ScriptError::Steel(e.to_string()))
}
