use std::any::Any;
use std::ffi::CString;

use dex_core::messages::{
    ActionBody, ActionHandler, RequestBody, RequestableDyn, action_to_python, request_to_python,
};
use dex_core::prelude::*;
use pyo3::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::layouts::error::ErrorLayout;

/// A node backed by a script-defined object.
pub struct DynamicNode {
    obj: Option<Py<PyAny>>,
}

impl DynamicNode {
    /// Call an optional `name(ctx)` hook on the script object, if it defines one.
    fn call_hook(&self, name: &str, ctx: NodeContext) {
        Python::attach(|py| {
            let Some(obj) = &self.obj else { return };
            let bound = obj.bind(py);
            if !bound.hasattr(name).unwrap_or(false) {
                return;
            }
            let called =
                PyNodeContext::enter(py, ctx, |pyctx| bound.call_method1(name, (pyctx.clone(),)));
            match called {
                Ok(Err(e)) => eprintln!("dynamic node `{name}` raised: {e}"),
                Ok(Ok(_)) => {}
                Err(e) => eprintln!("dynamic node `{name}` could not be called: {e}"),
            }
        });
    }

    /// The wrapped script object, if it was captured or restored.
    pub fn object(&self) -> Option<&Py<PyAny>> {
        self.obj.as_ref()
    }

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

/// A script object gets the same `Node` surface a Rust node has. Each hook is optional.
#[utils::dynamic_node(skip)]
impl Node for DynamicNode {
    /// The script's own `type_name()`, else its class name.
    fn type_name(&self) -> String {
        Python::attach(|py| {
            let Some(obj) = &self.obj else {
                return "Dynamic Node".to_owned();
            };
            let bound = obj.bind(py);
            if let Ok(true) = bound.hasattr("type_name")
                && let Ok(name) = bound.call_method0("type_name")
                && let Ok(name) = name.extract::<String>()
            {
                return name;
            }
            bound
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "Dynamic Node".to_owned())
        })
    }

    fn deref_target(&self) -> Option<NodeUid> {
        Python::attach(|py| {
            let bound = self.obj.as_ref()?.bind(py);
            if !bound.hasattr("deref_target").unwrap_or(false) {
                return None;
            }
            match bound.call_method0("deref_target") {
                Ok(v) => v.extract::<NodeHandle>().ok().map(|h| h.0),
                Err(e) => {
                    eprintln!("dynamic node `deref_target` raised: {e}");
                    None
                }
            }
        })
    }

    fn tick(&self, ctx: NodeContext) {
        self.call_hook("tick", ctx);
    }

    fn on_delete(&self, ctx: NodeContext) {
        self.call_hook("on_delete", ctx);
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        use crate::scripting::DynDraw;

        let Some(obj) = &self.obj else {
            return draw_error(&mut ctx, "dynamic node could not be restored");
        };
        match crate::scripting::draw_python_node(obj, &mut ctx) {
            DynDraw::Drawn(result) => result,
            DynDraw::NoDraw => draw_error(&mut ctx, "dynamic object has no `draw` method"),
            DynDraw::Error(e) => draw_error(&mut ctx, &format!("dynamic draw error: {e}")),
        }
    }
}

/**
    Dispatch an incoming action to the script object's `handle_action`.
    The handler mutates a **deep copy**: the workspace clones a node before handing it an action.
    A handler returns `NotImplemented` to decline, and the action is forwarded on.
*/
impl ActionHandler for DynamicNode {
    fn handle_action(
        &mut self,
        body: Box<dyn ActionBody>,
        ctx: NodeContext,
    ) -> Option<Box<dyn ActionBody>> {
        let updated = Python::attach(|py| {
            let bound = self.obj.as_ref()?.bind(py);
            if !bound.hasattr("handle_action").unwrap_or(false) {
                return None;
            }
            let action = action_to_python(&*body, py)?.ok()?;

            let target = deepcopy(py, bound).unwrap_or_else(|_| bound.clone());
            let result = PyNodeContext::enter(py, ctx, |pyctx| {
                target.call_method1("handle_action", (action, pyctx.clone()))
            })
            .ok()?;

            match result {
                Ok(v) if v.is(py.NotImplemented()) => None,
                Ok(_) => Some(target.unbind()),
                Err(e) => {
                    eprintln!("dynamic node `handle_action` raised: {e}");
                    None
                }
            }
        });

        match updated {
            Some(new_obj) => {
                self.obj = Some(new_obj);
                None
            }
            // Declined, absent, or raised: forward it on.
            None => Some(body),
        }
    }
}

/// Dispatch an incoming request to the script object's `request`, boxing what
/// it returns as the request's declared Rust response type.
impl RequestableDyn for DynamicNode {
    fn request_dyn(
        &self,
        body: Box<dyn RequestBody>,
        ctx: NodeContext,
    ) -> Result<Box<dyn Any>, Box<dyn RequestBody>> {
        let answered = Python::attach(|py| {
            let obj = self.obj.as_ref()?;
            let bound = obj.bind(py);
            if !bound.hasattr("request").unwrap_or(false) {
                return None;
            }
            let (entry, request) = request_to_python(&*body, py)?;
            let request = request.ok()?;

            let result = PyNodeContext::enter(py, ctx, |pyctx| {
                bound.call_method1("request", (request, pyctx.clone()))
            })
            .ok()?;

            let value = match result {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("dynamic node `request` raised: {e}");
                    return None;
                }
            };
            if value.is(py.NotImplemented()) {
                return None;
            }
            match (entry.response_from_python)(&value) {
                Ok(boxed) => Some(boxed),
                Err(e) => {
                    eprintln!("dynamic node `request` returned an unusable value: {e}");
                    None
                }
            }
        });

        answered.ok_or(body)
    }
}

/// A deep copy of a script object, so a handler cannot mutate a past version.
fn deepcopy<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("copy")?.call_method1("deepcopy", (obj,))
}

/// Draw an error node filling the current constraints.
fn draw_error(ctx: &mut DrawContext, message: &str) -> DrawResult {
    let error = ErrorLayout::message(message.to_owned());
    let constraints = ctx.constraints;
    ctx.draw_node(&error, constraints)
}

// `DynamicNode::from_python` is called by the coercion in `crate::scripting`.

/// Provide `cloudpickle` as a module
fn pickler<'py>(py: Python<'py>) -> Bound<'py, PyModule> {
    PyModule::from_code(
        py,
        &CString::new(include_str!("../pydeps/cloudpickle.py")).unwrap(),
        &CString::new("cloudpickle.py").unwrap(),
        &CString::new("cloudpickle").unwrap(),
    )
    .expect("cloudpickle module should construct!")
}

/// `dumps(obj)` as bytes, via [`pickler`].
fn pickle_dumps<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Vec<u8>> {
    pickler(py).call_method1("dumps", (obj,))?.extract()
}

/// `loads(bytes)`, via [`pickler`].
fn pickle_loads<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    pickler(py).call_method1("loads", (pyo3::types::PyBytes::new(py, bytes),))
}
