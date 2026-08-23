// Allows macro-generated `::dex_dynamic::...` paths resolve within this crate itself.
extern crate self as dex_dynamic;

/// Runtime dependencies referenced by macro-generated code.
pub mod __rt {
    pub use inventory;
    pub use pyo3;
    pub use steel;
}

use pyo3::prelude::*;
use pyo3::types::PyModule;
use steel::steel_vm::engine::Engine;

/// The contribution of a singular type or function to each environment.
pub struct DynamicBinding {
    /// A human-readable identifier for the bound item.
    pub name: &'static str,
    /// Installs this item into the `dex` Python module.
    pub register_python: fn(&Bound<'_, PyModule>) -> PyResult<()>,
    /// Installs this item into a Steel engine.
    pub register_steel: fn(&mut Engine),
}

inventory::collect!(DynamicBinding);

/// Assemble the `dex` Python module from every registered binding.
pub fn build_python_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let module = PyModule::new(py, "dex")?;
    for binding in inventory::iter::<DynamicBinding> {
        (binding.register_python)(&module)?;
    }
    Ok(module)
}

/// Assemble a fresh Steel engine with every registered binding installed.
pub fn build_steel_engine() -> Engine {
    let mut engine = Engine::new();
    for binding in inventory::iter::<DynamicBinding> {
        (binding.register_steel)(&mut engine);
    }
    engine
}
