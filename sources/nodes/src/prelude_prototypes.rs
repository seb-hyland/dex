//! Nodes a workspace's own prelude offers, alongside the built-in primitives.

use std::sync::{Arc, Mutex};

use dex_core::prelude::*;
use pyo3::prelude::*;

/// What a prototype is placed at when the prelude names no size.
pub const DEFAULT_SIZE: Vector = Vector { x: 160.0, y: 40.0 };

/// One offered node: what to call it, what it is, and how big to place it.
pub type Offer = (String, Arc<dyn Node>, Vector);

/// One entry as the prelude wrote it: a name, a size, and the thing itself.
struct Entry {
    name: String,
    /// A node, or a callable that builds one when handed a workspace.
    source: Py<PyAny>,
    size: Vector,
}

/**
    The list a prelude adds to.

    Published as `dex.prelude_prototypes`.
*/
#[pyo3::pyclass(from_py_object, module = "dex")]
#[derive(Clone, Default)]
pub struct PreludePrototypes {
    offers: Arc<Mutex<Vec<Entry>>>,
}

#[pyo3::pymethods]
impl PreludePrototypes {
    /**
        Offer a node in the sidebar as `name`, placed at `size` if one is given.

        `node` is either the node itself or — better, for anything with children
        — a callable taking a workspace and returning one. The prelude is run
        again before *every* lambda, and a factory is only ever called when the
        sidebar is actually asking what is on offer, so a prototype that inserts
        dropdowns and sensors does not seed a fresh set of them on each run.

        ```python
        dex.prelude_prototypes.add("Data explorer", build_explorer, (760.0, 560.0))
        ```
    */
    #[pyo3(signature = (name, node, size=None))]
    fn add(
        &self,
        name: String,
        node: pyo3::Bound<'_, pyo3::PyAny>,
        size: Option<(f32, f32)>,
    ) -> pyo3::PyResult<()> {
        let size = size.map(|(x, y)| Vector { x, y }).unwrap_or(DEFAULT_SIZE);
        if let Ok(mut offers) = self.offers.lock() {
            offers.push(Entry {
                name,
                source: node.unbind(),
                size,
            });
        }
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.offers.lock().map(|offers| offers.len()).unwrap_or(0)
    }
}

impl PreludePrototypes {
    /**
        Everything added so far, with every factory called against `ws`.

        A factory that raises is reported rather than dropped: an offer that
        silently fails to appear is a worse bug than one that says why.
    */
    fn taken(&self, py: Python<'_>, ws: &Bound<'_, PyAny>) -> (Vec<Offer>, Option<String>) {
        let Ok(entries) = self.offers.lock() else {
            return (Vec::new(), None);
        };
        let mut offers = Vec::with_capacity(entries.len());
        let mut error = None;
        for entry in entries.iter() {
            let source = entry.source.bind(py);
            let built = if source.is_callable() {
                match source.call1((ws,)) {
                    Ok(node) => node,
                    Err(e) => {
                        error.get_or_insert_with(|| {
                            format!("building {:?} raised: {e}", entry.name)
                        });
                        continue;
                    }
                }
            } else {
                source.clone()
            };
            offers.push((
                entry.name.clone(),
                crate::scripting::to_dyn_node_py(&built),
                entry.size,
            ));
        }
        (offers, error)
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
    dex_core::stubs::StubGlobal {
        name: "prelude_prototypes",
        ty: "PreludePrototypes",
        doc: "The nodes this workspace's prelude offers in the sidebar.",
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
        doc: "Offer `node` in the sidebar as `name`, placed at `size` if one is given. \
`node` is the node itself, or a callable taking a workspace and returning one.",
        params: &[
            dex_core::stubs::StubField { name: "name", ty: "String" },
            dex_core::stubs::StubField { name: "node", ty: "Any" },
            dex_core::stubs::StubField { name: "size", ty: "Option<(f32, f32)>" },
        ],
        returns: "",
        is_static: false,
    }
}

/**
    Run `prelude` against `handle` and hand back the nodes it offered.

    `handle` is seeded as `dex.ws`, exactly as it is for a lambda, so a prelude
    can build a prototype's children — the dropdowns and sensors a real view is
    made of — the same way an example's `build(ws, ...)` does. The inserts it
    queues reach the workspace through the same channel as the offer itself, and
    in the order they were made, so a template is seated after the children it
    names.

    A prelude that will not run is reported rather than swallowed: the sidebar
    is often the first place anyone looks after editing it, so a syntax error
    belongs there and not only in the next lambda that tries to run.
*/
pub fn read(prelude: &str, handle: &WorkspaceActionHandle) -> (Vec<Offer>, Option<String>) {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use std::ffi::CString;

    pyo3::Python::attach(|py| {
        let failed = |e: PyErr| (Vec::new(), Some(e.to_string()));
        let dex_mod = match dex_dynamic::build_python_module(py) {
            Ok(module) => module,
            Err(e) => return failed(e),
        };
        let ws = match Bound::new(py, handle.clone()) {
            Ok(ws) => ws,
            Err(e) => return failed(e),
        };
        if let Err(e) = dex_mod.add("ws", &ws) {
            return failed(e);
        }
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
            Ok(list) => {
                let ws = ws.into_any();
                list.taken(py, &ws)
            }
            Err(e) => failed(e),
        }
    })
}
