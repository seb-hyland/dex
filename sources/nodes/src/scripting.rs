use std::ffi::{CString, NulError};

use dex_core::messages::TypedRequestBody;
use dex_core::prelude::*;

use arrow::array::RecordBatch;

use crate::{
    composites::lambda::ComputeParam,
    layouts::{error::ErrorLayout, pending::PendingLayout},
    primitives::{
        dynamic::DynamicNode,
        nothing::Nothing,
        table::Table,
        text::{Label, LabelEditable},
    },
};

/// Initialise the Python interpreter.
pub fn init_python() {
    pyo3::Python::initialize();
}

/// A scripting language that dex binds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptLang {
    Python,
    Steel,
}

/// Possible output cases for a transform's return value.
pub enum ScriptOutput {
    /// The script returned void.
    Nothing,
    Node(Arc<dyn Node>),
    Handle(NodeUid),
}

/// A resolved argument value, injected into scripts by type.
#[derive(Clone, Debug)]
pub enum ScriptValue {
    Nothing,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Node(NodeUid),
    Table(RecordBatch),
}

impl ScriptValue {
    /// A plain-text rendering (for previews and string-typed sinks).
    pub fn display(&self) -> String {
        match self {
            ScriptValue::Str(s) => s.clone(),
            ScriptValue::Int(i) => i.to_string(),
            ScriptValue::Float(f) => f.to_string(),
            ScriptValue::Bool(b) => b.to_string(),
            ScriptValue::Node(_) => "⟨node⟩".to_owned(),
            ScriptValue::Nothing => "⟨nothing⟩".to_owned(),
            ScriptValue::Table(rb) => {
                format!("⟨table {}×{}⟩", rb.num_rows(), rb.num_columns())
            }
        }
    }
}

#[derive(Clone)]
struct SteelTable(#[allow(dead_code)] RecordBatch);

impl steel::rvals::Custom for SteelTable {}

/// "My value is really this other node's value."
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ValueDelegate;

impl TypedRequestBody for ValueDelegate {
    type Response = Option<NodeUid>;
}

/// A wired argument resolved to its leaf value.
pub struct ResolvedArg {
    /// The leaf's value, if it is a value-bearing node.
    pub value: ScriptValue,
    /// A change token — differs iff the resolved value changed.
    pub version: u64,
    /// Whether the delegation chain passed through a pending marker.
    pub pending: bool,
}

/// Follow `start`'s value-delegation chain to its leaf.
pub fn resolve_arg(ws: &Workspace, start: NodeUid) -> ResolvedArg {
    let mut cur = start;
    let mut pending = false;
    loop {
        if ws
            .get_node(cur)
            .is_some_and(|n| n.as_ref().as_any_ref().is::<PendingLayout>())
        {
            pending = true;
        }
        match ws.send_request(cur, ValueDelegate).flatten() {
            Some(next) => cur = next,
            None => break,
        }
    }
    // A value-bearing leaf resolves to its value; the empty node to nothing;
    // any other node to a reference the script can use as a node.
    let value = ws
        .get_node(cur)
        .map(|n| {
            node_to_value(&*n).unwrap_or_else(|| {
                if n.as_ref().as_any_ref().is::<Nothing>() {
                    ScriptValue::Nothing
                } else {
                    ScriptValue::Node(cur)
                }
            })
        })
        .unwrap_or(ScriptValue::Nothing);
    ResolvedArg {
        value: if pending { ScriptValue::Nothing } else { value },
        version: ws.node_version(cur),
        pending,
    }
}

/// The single place a value-bearing node becomes a primitive [`ScriptValue`].
/// A node not handled here is passed to the script by reference (as a node).
pub fn node_to_value(node: &dyn Node) -> Option<ScriptValue> {
    let any = node.as_any_ref();
    if let Some(l) = any.downcast_ref::<Label>() {
        return Some(ScriptValue::Str(l.text.clone()));
    }
    if let Some(l) = any.downcast_ref::<LabelEditable>() {
        return Some(ScriptValue::Str(l.resolved_text()));
    }
    if let Some(p) = any.downcast_ref::<ComputeParam>() {
        return Some(ScriptValue::Str(p.value.clone()));
    }
    if let Some(t) = any.downcast_ref::<Table>() {
        return Some(ScriptValue::Table(t.batch().clone()));
    }
    None
}

/// The one canonical mapping from a Python value to a node.
pub fn to_dyn_node_py(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Arc<dyn Node> {
    use pyo3::prelude::*;
    if obj.is_none() {
        return Arc::new(Nothing);
    }
    // `bool` before `int`: in Python `bool` extracts as `int` too.
    if let Ok(v) = obj.extract::<bool>() {
        return Arc::new(Label::new(v.to_string()));
    }
    if let Ok(v) = obj.extract::<i64>() {
        return Arc::new(Label::new(v.to_string()));
    }
    if let Ok(v) = obj.extract::<f64>() {
        return Arc::new(Label::new(v.to_string()));
    }
    if let Ok(v) = obj.extract::<String>() {
        return Arc::new(Label::new(v));
    }
    for extractor in dex_dynamic::__rt::inventory::iter::<NodeExtractor> {
        if let Some(node) = (extractor.from_python)(obj) {
            return node;
        }
    }
    Arc::new(DynamicNode::from_python(obj))
}

/// The Steel counterpart of [`to_dyn_node_py`]. Steel has no dynamically-drawn
/// nodes, so an unrecognised value resolves to an error node.
pub fn to_dyn_node_steel(val: &steel::rvals::SteelVal) -> Arc<dyn Node> {
    use steel::rvals::SteelVal;
    match val {
        SteelVal::Void => return Arc::new(Nothing),
        SteelVal::BoolV(v) => return Arc::new(Label::new(v.to_string())),
        SteelVal::IntV(v) => return Arc::new(Label::new(v.to_string())),
        SteelVal::NumV(v) => return Arc::new(Label::new(v.to_string())),
        SteelVal::StringV(v) => return Arc::new(Label::new(v.to_string())),
        _ => {}
    }
    for extractor in dex_dynamic::__rt::inventory::iter::<NodeExtractor> {
        if let Some(node) = (extractor.from_steel)(val) {
            return node;
        }
    }
    Arc::new(ErrorLayout::message(
        "script returned an unsupported Steel value".to_owned(),
    ))
}

/// A scoped, script-facing handle to a live [`DrawContext`]. The pointer inside
/// is valid only for the duration of the synchronous `draw` call it is passed
/// into; it is nulled afterward so a stashed handle can never dangle.
#[pyo3::pyclass(unsendable, name = "DrawContext")]
pub struct PyDrawContext {
    ctx: std::cell::Cell<*mut DrawContext<'static>>,
    region: std::cell::Cell<Option<ScreenRegion>>,
}

impl PyDrawContext {
    fn new(ctx: &mut DrawContext) -> Self {
        // Erase the borrow to a raw pointer.
        // Only used during the synchronous call, then invalidated.
        #[allow(clippy::unnecessary_cast)]
        let ptr = ctx as *mut DrawContext as *mut DrawContext<'static>;
        Self {
            ctx: std::cell::Cell::new(ptr),
            region: std::cell::Cell::new(None),
        }
    }

    fn with_ctx<R>(&self, f: impl FnOnce(&mut DrawContext<'_>) -> R) -> Option<R> {
        let ptr = self.ctx.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: the pointer targets the stack `DrawContext` of the in-progress
        // `draw` call and is nulled the moment that call returns.
        Some(f(unsafe { &mut *ptr }))
    }

    fn invalidate(&self) {
        self.ctx.set(std::ptr::null_mut());
    }

    /// Fold a freshly drawn region into the running total the node reports.
    fn merge_region(&self, region: Option<ScreenRegion>) {
        let merged = match (self.region.get(), region) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        };
        self.region.set(merged);
    }
}

#[pyo3::pymethods]
impl PyDrawContext {
    /// Draw `node` filling the context's current area, accumulating the region drawn.
    fn draw_node(&self, node: pyo3::Bound<'_, pyo3::PyAny>) {
        let arc = to_dyn_node_py(&node);
        self.with_ctx(|ctx| {
            let constraints = ctx.constraints;
            let region = ctx.draw_node(&*arc, constraints).region();
            self.merge_region(region);
        });
    }

    /// Draw `node` into a sub-box at node-local `(x, y)` with size `w x h`, clipped to it.
    fn draw_node_at(&self, node: pyo3::Bound<'_, pyo3::PyAny>, x: f32, y: f32, w: f32, h: f32) {
        let arc = to_dyn_node_py(&node);
        self.with_ctx(|ctx| {
            let constraints = DrawConstraints {
                pos: ctx.constraints.pos + Vector { x, y },
                x: Some(AxisConstraint::Exactly(w)),
                y: Some(AxisConstraint::Exactly(h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            };
            let region = ctx.draw_node(&*arc, constraints).region();
            self.merge_region(region);
        });
    }

    /// Available width, if the parent bounded it.
    fn avail_width(&self) -> Option<f32> {
        self.with_ctx(|ctx| ctx.constraints.x.map(|a| a.provided_value()))
            .flatten()
    }

    /// Available height, if the parent bounded it.
    fn avail_height(&self) -> Option<f32> {
        self.with_ctx(|ctx| ctx.constraints.y.map(|a| a.provided_value()))
            .flatten()
    }
}

/// The result of asking a dynamic object to draw itself.
pub enum DynDraw {
    /// The object drew; here is the region it occupied (if any).
    Drawn(Option<ScreenRegion>),
    /// The object exposes no `draw` method.
    NoDraw,
    /// Calling `draw` raised an exception.
    Error(String),
}

/// Call a Python object's `draw(ctx)` with a scoped handle to `ctx`.
pub fn draw_python_node(obj: &pyo3::Py<pyo3::PyAny>, ctx: &mut DrawContext) -> DynDraw {
    use pyo3::prelude::*;
    Python::attach(|py| {
        let bound = obj.bind(py);
        match bound.hasattr("draw") {
            Ok(true) => {}
            Ok(false) => return DynDraw::NoDraw,
            Err(e) => return DynDraw::Error(e.to_string()),
        }
        let pyctx = match Bound::new(py, PyDrawContext::new(ctx)) {
            Ok(p) => p,
            Err(e) => return DynDraw::Error(e.to_string()),
        };
        let result = bound.call_method1("draw", (pyctx.clone(),));
        let drawn_region = pyctx.borrow().region.get();
        // Invalidate before touching `ctx` again so no live handle aliases it.
        pyctx.borrow().invalidate();
        match result {
            Ok(ret) if !ret.is_none() => {
                let arc = to_dyn_node_py(&ret);
                let constraints = ctx.constraints;
                DynDraw::Drawn(ctx.draw_node(&*arc, constraints).region())
            }
            Ok(_) => DynDraw::Drawn(drawn_region),
            Err(e) => DynDraw::Error(e.to_string()),
        }
    })
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

/// Run `source` in `lang` with `handle` as context and `args` seeded as globals.
pub fn run_script(
    lang: ScriptLang,
    source: &str,
    py_prelude: &str,
    handle: &WorkspaceActionHandle,
    args: &[(String, ScriptValue)],
) -> Result<ScriptOutput, ScriptError> {
    match lang {
        ScriptLang::Python => run_python(source, py_prelude, handle, args),
        ScriptLang::Steel => run_steel(source, handle, args),
    }
}

fn run_python(
    source: &str,
    prelude: &str,
    handle: &WorkspaceActionHandle,
    args: &[(String, ScriptValue)],
) -> Result<ScriptOutput, ScriptError> {
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
        // Present the exec namespace as the `__main__` module.
        globals.set_item("__name__", "__main__").map_err(map_err)?;
        globals.set_item("dex", &dex_mod).map_err(map_err)?;

        // Seed each argument as a global.
        for (name, value) in args {
            match value {
                ScriptValue::Str(s) => globals.set_item(name, s),
                ScriptValue::Int(i) => globals.set_item(name, *i),
                ScriptValue::Float(f) => globals.set_item(name, *f),
                ScriptValue::Bool(b) => globals.set_item(name, *b),
                ScriptValue::Node(uid) => {
                    let handle = Bound::new(py, NodeHandle(*uid)).map_err(map_err)?;
                    globals.set_item(name, handle)
                }
                ScriptValue::Table(rb) => {
                    use arrow::pyarrow::ToPyArrow;
                    let obj = rb.to_pyarrow(py).map_err(map_err)?;
                    globals.set_item(name, obj)
                }
                ScriptValue::Nothing => globals.set_item(name, ()),
            }
            .map_err(map_err)?;
        }

        // Run the prelude into the shared namespace, then the source.
        let prelude = CString::new(prelude)?;
        py.run(prelude.as_c_str(), Some(&globals), Some(&globals))
            .map_err(map_err)?;
        py.run(code.as_c_str(), Some(&globals), Some(&globals))
            .map_err(map_err)?;
        let Some(transform) = globals.get_item("transform").map_err(map_err)? else {
            return Err(ScriptError::Python(
                "script must define a `transform` function".to_owned(),
            ));
        };
        let result = transform.call0().map_err(map_err)?;
        Ok(extract_python(&result))
    })
}

fn run_steel(
    source: &str,
    handle: &WorkspaceActionHandle,
    args: &[(String, ScriptValue)],
) -> Result<ScriptOutput, ScriptError> {
    let mut engine = dex_dynamic::build_steel_engine();
    let to_err = |e: steel::rerrs::SteelErr| ScriptError::Steel(e.to_string());

    // Seed the `ws` global.
    engine
        .register_external_value("ws", handle.clone())
        .map_err(to_err)?;

    // Seed each argument as a global bound to its native value.
    for (name, value) in args {
        match value {
            ScriptValue::Str(s) => engine.register_external_value(name, s.clone()),
            ScriptValue::Int(i) => engine.register_external_value(name, *i),
            ScriptValue::Float(f) => engine.register_external_value(name, *f),
            ScriptValue::Bool(b) => engine.register_external_value(name, *b),
            ScriptValue::Node(uid) => engine.register_external_value(name, NodeHandle(*uid)),
            // Steel has no record-batch type; pass it as an opaque value scripts can
            // hold and forward but not deconstruct (record batches are a Python-side
            // feature).
            ScriptValue::Table(rb) => engine.register_external_value(name, SteelTable(rb.clone())),
            ScriptValue::Nothing => engine.register_external_value(name, ()),
        }
        .map_err(to_err)?;
    }

    // Run the source (defining `transform`), then call it via the engine env — a
    // fresh `(transform)` run would not resolve the identifier.
    engine.run(source.to_owned()).map_err(to_err)?;
    let result = engine
        .call_function_by_name_with_args("transform", vec![])
        .map_err(to_err)?;
    Ok(extract_steel(&result))
}

/// Whether `name` is a valid script identifier (so it can be seeded as a global).
pub fn is_valid_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn extract_python(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> ScriptOutput {
    use pyo3::prelude::*;

    // A handle stays a handle so its live workspace node is reused; everything
    // else flows through the shared value->node mapping.
    if let Ok(handle) = obj.extract::<NodeHandle>() {
        return ScriptOutput::Handle(handle.0);
    }
    ScriptOutput::Node(to_dyn_node_py(obj))
}

fn extract_steel(val: &steel::rvals::SteelVal) -> ScriptOutput {
    use steel::rvals::FromSteelVal;

    if let Ok(handle) = NodeHandle::from_steelval(val) {
        return ScriptOutput::Handle(handle.0);
    }
    ScriptOutput::Node(to_dyn_node_steel(val))
}
