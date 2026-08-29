// Allows macro-generated `::dex_dynamic::...` paths resolve within this crate itself.
extern crate self as dex_dynamic;

/// Runtime dependencies referenced by macro-generated code.
pub mod __rt {
    pub use inventory;
    pub use pyo3;
}

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// The contribution of a singular type or function to the Python environment.
pub struct DynamicBinding {
    /// A human-readable identifier for the bound item.
    pub name: &'static str,
    /// Installs this item into the `dex` Python module.
    pub register_python: fn(&Bound<'_, PyModule>) -> PyResult<()>,
}

inventory::collect!(DynamicBinding);

/**
    Assemble the `dex` Python module from every registered binding.

    Two bindings sharing a name would have the second silently shadow the first
    as a module attribute, so that is reported rather than left to be discovered
    as a mysteriously absent class.
*/
pub fn build_python_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let module = PyModule::new(py, "dex")?;

    // Generated `__reduce__` implementations name `dex._rebuild`, and pickle
    // resolves that by importing `dex` — so the module must be in `sys.modules`.
    py.import("sys")?
        .getattr("modules")?
        .set_item("dex", &module)?;

    let mut seen: Vec<&'static str> = Vec::new();
    for binding in inventory::iter::<DynamicBinding> {
        if seen.contains(&binding.name) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "two bindings are both named `{}`; rename one so each is reachable \
                 as a distinct `dex` attribute",
                binding.name
            )));
        }
        seen.push(binding.name);
        (binding.register_python)(&module)?;
    }

    Ok(module)
}
