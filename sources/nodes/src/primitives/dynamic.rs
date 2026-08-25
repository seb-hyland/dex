use dex_core::prelude::*;
use pyo3::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::layouts::error::ErrorLayout;

/// A node backed by a script-defined object.
pub struct DynamicNode {
    obj: Option<Py<PyAny>>,
}

impl DynamicNode {
    /// Wrap a Python object returned by a script.
    pub fn from_python(obj: &Bound<'_, PyAny>) -> Self {
        Self {
            obj: Some(obj.clone().unbind()),
        }
    }
}

impl Clone for DynamicNode {
    fn clone(&self) -> Self {
        // `Py::clone` panics off the interpreter, and node clones happen on the
        // main thread (off-GIL), so attach for the ref-count bump.
        let obj = self
            .obj
            .as_ref()
            .map(|o| Python::attach(|py| o.clone_ref(py)));
        Self { obj }
    }
}

impl Serialize for DynamicNode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Pickle to bytes; an empty buffer marks an object that could not be
        // captured (persistence is best-effort for now).
        let bytes = self
            .obj
            .as_ref()
            .and_then(|o| Python::attach(|py| pickle_dumps(py, o.bind(py)).ok()))
            .unwrap_or_default();
        bytes.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DynamicNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let obj = if bytes.is_empty() {
            None
        } else {
            Python::attach(|py| pickle_loads(py, &bytes).ok().map(|o| o.unbind()))
        };
        Ok(Self { obj })
    }
}

impl utils::Reset for DynamicNode {
    #[inline(always)]
    fn reset(&self) {}
}

#[utils::dynamic_node(skip)]
impl Node for DynamicNode {
    fn type_name(&self) -> String {
        "Dynamic Node".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        use crate::scripting::DynDraw;

        let Some(obj) = &self.obj else {
            return draw_error(&mut ctx, "dynamic node could not be restored");
        };
        match crate::scripting::draw_python_node(obj, &mut ctx) {
            DynDraw::Drawn(region) => DrawResult::Complete { region },
            DynDraw::NoDraw => draw_error(&mut ctx, "dynamic object has no `draw` method"),
            DynDraw::Error(e) => draw_error(&mut ctx, &format!("dynamic draw error: {e}")),
        }
    }
}

defhandlers! { DynamicNode {} }

/// Draw an error node filling the current constraints.
fn draw_error(ctx: &mut DrawContext, message: &str) -> DrawResult {
    let error = ErrorLayout::message(message.to_owned());
    let constraints = ctx.constraints;
    ctx.draw_node(&error, constraints)
}

// `DynamicNode::from_python` is called by the coercion in `crate::scripting`.

/// `pickle.dumps(obj)` as bytes.
fn pickle_dumps<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Vec<u8>> {
    let pickle = py.import("pickle")?;
    pickle.call_method1("dumps", (obj,))?.extract()
}

/// `pickle.loads(bytes)`.
fn pickle_loads<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let pickle = py.import("pickle")?;
    pickle.call_method1("loads", (pyo3::types::PyBytes::new(py, bytes),))
}
