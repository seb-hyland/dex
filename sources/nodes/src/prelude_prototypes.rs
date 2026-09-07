//! Nodes a workspace's own prelude offers, alongside the built-in primitives.

use std::sync::{Arc, Mutex};

use dex_core::prelude::*;

/// What a prototype is placed at when the prelude names no size.
pub const DEFAULT_SIZE: Vector = Vector { x: 160.0, y: 40.0 };

/// One offered node: what to call it, what it is, and how big to place it.
pub type Offer = (String, Arc<dyn Node>, Vector);

/**
    The list a prelude adds to.

    Published as `dex.prelude_prototypes`.
*/
#[pyo3::pyclass(from_py_object, module = "dex")]
#[derive(Clone, Default)]
pub struct PreludePrototypes {
    offers: Arc<Mutex<Vec<Offer>>>,
}

#[pyo3::pymethods]
impl PreludePrototypes {
    /// Offer `node` in the sidebar as `name`, placed at `size` if one is given.
    #[pyo3(signature = (name, node, size=None))]
    fn add(
        &self,
        name: String,
        node: pyo3::Bound<'_, pyo3::PyAny>,
        size: Option<(f32, f32)>,
    ) -> pyo3::PyResult<()> {
        let node = crate::scripting::to_dyn_node_py(&node);
        let size = size.map(|(x, y)| Vector { x, y }).unwrap_or(DEFAULT_SIZE);
        if let Ok(mut offers) = self.offers.lock() {
            offers.push((name, node, size));
        }
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.offers.lock().map(|offers| offers.len()).unwrap_or(0)
    }
}

impl PreludePrototypes {
    /// Everything added so far.
    fn taken(&self) -> Vec<Offer> {
        self.offers
            .lock()
            .map(|offers| offers.clone())
            .unwrap_or_default()
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_dynamic::DynamicBinding {
        name: "PreludePrototypes",
        register_python: |m| {
            use dex_dynamic::__rt::pyo3::prelude::*;
            use dex_dynamic::__rt::pyo3::types::PyModuleMethods;
            m.add_class::<PreludePrototypes>()?;
            // The list itself, which is what a prelude actually reaches for.
            m.add(
                "prelude_prototypes",
                Py::new(m.py(), PreludePrototypes::default())?,
            )
        },
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubClass {
        name: "PreludePrototypes",
        doc: "The nodes this workspace's prelude offers in the sidebar.",
        fields: &[],
        constructible: false,
        variants: &[],
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubMethod {
        owner: "PreludePrototypes",
        name: "add",
        doc: "Offer `node` in the sidebar as `name`, placed at `size` if one is given.",
        params: &[
            dex_core::stubs::StubField { name: "name", ty: "String" },
            dex_core::stubs::StubField { name: "node", ty: "Arc<dyn Node>" },
            dex_core::stubs::StubField { name: "size", ty: "Option<(f32, f32)>" },
        ],
        returns: "",
        is_static: false,
    }
}

/**
    Run `prelude` and hand back the nodes it offered.

    A prelude that will not run is reported rather than swallowed: the sidebar
    is often the first place anyone looks after editing it, so a syntax error
    belongs there and not only in the next lambda that tries to run.
*/
pub fn read(prelude: &str) -> (Vec<Offer>, Option<String>) {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use std::ffi::CString;

    pyo3::Python::attach(|py| {
        let failed = |e: PyErr| (Vec::new(), Some(e.to_string()));
        let dex_mod = match dex_dynamic::build_python_module(py) {
            Ok(module) => module,
            Err(e) => return failed(e),
        };
        let globals = PyDict::new(py);
        if let Err(e) = globals
            .set_item("__name__", "__main__")
            .and_then(|()| globals.set_item("dex", &dex_mod))
        {
            return failed(e);
        }
        let Ok(code) = CString::new(prelude) else {
            return (
                Vec::new(),
                Some("the prelude contains an interior NUL byte".to_owned()),
            );
        };
        if let Err(e) = py.run(code.as_c_str(), Some(&globals), Some(&globals)) {
            return failed(e);
        }

        let offered = dex_mod
            .getattr("prelude_prototypes")
            .and_then(|list| Ok(list.extract::<PreludePrototypes>()?));
        match offered {
            Ok(list) => (list.taken(), None),
            Err(e) => failed(e),
        }
    })
}
