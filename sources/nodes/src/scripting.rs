use std::ffi::{CString, NulError};

use dex_core::messages::TypedRequestBody;
use dex_core::prelude::*;

use arrow::array::RecordBatch;

use crate::{
    layouts::{child::LayoutChild, pending::PendingLayout, scroll::ScrollLayout},
    primitives::{
        boolean::Bool,
        dynamic::DynamicNode,
        nothing::Nothing,
        number::{Float, Integer},
        table::{ArrowData, Table},
        text::{Label, LabelEditable},
    },
};

/// Initialise the Python interpreter, against the default environment.
///
/// The environment has to be adopted here rather than left to whoever asks
/// first: `sys.path` is the interpreter's, so it must be set before anything
/// imports anything.
pub fn init_python() {
    pyo3::Python::initialize();
    crate::settings::adopt_default_venv();
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
    /// The Python type a script sees this value as, for a checkout's header.
    pub fn python_type(&self) -> &'static str {
        match self {
            ScriptValue::Str(_) => "str",
            ScriptValue::Int(_) => "int",
            ScriptValue::Float(_) => "float",
            ScriptValue::Bool(_) => "bool",
            ScriptValue::Node(_) => "dex.NodeUid",
            // A pyarrow table; nothing the dex stubs describe.
            ScriptValue::Table(_) => "typing.Any",
            ScriptValue::Nothing => "None",
        }
    }

    /// A plain-text rendering (for previews and string-typed sinks).
    pub fn display(&self) -> String {
        match self {
            ScriptValue::Str(s) => s.clone(),
            ScriptValue::Int(i) => i.to_string(),
            ScriptValue::Float(f) => f.to_string(),
            ScriptValue::Bool(b) => b.to_string(),
            ScriptValue::Node(_) => "<node>".to_owned(),
            ScriptValue::Nothing => "<nothing>".to_owned(),
            ScriptValue::Table(rb) => {
                format!("(table {}×{})", rb.num_rows(), rb.num_columns())
            }
        }
    }
}

/// "My value is really this other node's value."
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ValueDelegate;

impl TypedRequestBody for ValueDelegate {
    type Response = Option<NodeUid>;
}

dex_core::defrequest!(
    /// The uid standing for this node's value: what a wire should point at to consume it.
    DataflowOutput: Option<NodeUid>
);

/// A wired argument resolved to its leaf value.
pub struct ResolvedArg {
    /// The leaf's value, if it is a value-bearing node.
    pub value: ScriptValue,
    /// The node the value was read from, once the delegation chain ran out.
    pub leaf: Option<NodeUid>,
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
    let leaf = ws.get_node(cur);
    let value = leaf
        .as_ref()
        .map(|n| {
            node_to_value(&**n).unwrap_or_else(|| {
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
        leaf: leaf.map(|_| cur),
        version: ws.version_of(cur),
        pending,
    }
}

/// One argument as the script sees it: the value, and where it was read from.
#[derive(Clone)]
pub struct ScriptArg {
    pub name: String,
    pub value: ScriptValue,
    /// The node the value was read from, or [`None`] when nothing was.
    pub source: Option<NodeUid>,
}

impl ScriptArg {
    /// An argument carrying a value and naming no node, for a caller that has
    /// only the value — every test, and anything not driven by a lambda's wires.
    pub fn detached(name: String, value: ScriptValue) -> ScriptArg {
        ScriptArg {
            name,
            value,
            source: None,
        }
    }
}

/// `dex.args` inside a transform: each argument's name, and the node it came from.
#[pyo3::pyclass(name = "Args")]
pub struct ScriptArgs {
    sources: std::collections::BTreeMap<String, NodeUid>,
}

impl ScriptArgs {
    /// The table for `args`, keeping only those that named a node.
    fn new(args: &[ScriptArg]) -> ScriptArgs {
        ScriptArgs {
            sources: args
                .iter()
                .filter_map(|arg| Some((arg.name.clone(), arg.source?)))
                .collect(),
        }
    }

    /// The one place a miss is reported, so both spellings fail the same way.
    fn lookup(&self, name: &str) -> pyo3::PyResult<NodeHandle> {
        self.sources
            .get(name)
            .copied()
            .map(NodeHandle)
            .ok_or_else(|| {
                let known = self
                    .sources
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                pyo3::exceptions::PyKeyError::new_err(match known.is_empty() {
                    true => format!("no argument named `{name}`; this transform has none wired"),
                    false => format!("no argument named `{name}`; wired arguments are: {known}"),
                })
            })
    }
}

#[pyo3::pymethods]
impl ScriptArgs {
    fn __getattr__(&self, name: &str) -> pyo3::PyResult<NodeHandle> {
        self.lookup(name)
    }

    fn __getitem__(&self, name: &str) -> pyo3::PyResult<NodeHandle> {
        self.lookup(name)
    }

    fn __contains__(&self, name: &str) -> bool {
        self.sources.contains_key(name)
    }

    fn __len__(&self) -> usize {
        self.sources.len()
    }

    /// The names that name a node, so a script can iterate rather than guess.
    fn keys(&self) -> Vec<String> {
        self.sources.keys().cloned().collect()
    }

    /// The node behind `name`, or `default` when there is none.
    #[pyo3(signature = (name, default=None))]
    fn get(&self, name: &str, default: Option<NodeHandle>) -> Option<NodeHandle> {
        self.sources.get(name).copied().map(NodeHandle).or(default)
    }

    fn __repr__(&self) -> String {
        format!("Args({})", self.keys().join(", "))
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_dynamic::DynamicBinding {
        name: "Args",
        register_python: |m| {
            use pyo3::types::PyModuleMethods;
            m.add_class::<ScriptArgs>()
        },
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubClass {
        name: "Args",
        doc: "Each of this transform's arguments, and the node its value was read from.",
        fields: &[],
        constructible: false,
        variants: &[],
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubGlobal {
        name: "prelude",
        // The workspace's own file, so no stub can know what is in it; a
        // checker is told the module exists and nothing about its members,
        // which is the truth and is what `Any` says.
        ty: "PreludeModule",
        doc: "The workspace prelude as a module, for reaching a name of its own that something nearer has shadowed.",
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubGlobal {
        name: "args",
        ty: "Args",
        doc: "The node behind each argument, for a transform that means to write back.",
    }
}

/*
    Both spellings are declared, so an editor accepts `dex.args.n` as readily as
    `dex.args["n"]`. A checker reads `__getattr__` as "this class has whatever
    attribute you name", which is the truth here: the names are the transform's
    own parameters, and no stub can know them ahead of time.
*/
dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubMethod {
        owner: "Args",
        name: "__getattr__",
        doc: "The node the argument called `name` read its value from.",
        params: &[dex_core::stubs::StubField { name: "name", ty: "String" }],
        returns: "NodeUid",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubMethod {
        owner: "Args",
        name: "__getitem__",
        doc: "The node the argument called `name` read its value from.",
        params: &[dex_core::stubs::StubField { name: "name", ty: "String" }],
        returns: "NodeUid",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubMethod {
        owner: "Args",
        name: "keys",
        doc: "The names that name a node, so a script can iterate rather than guess.",
        params: &[],
        returns: "Vec<String>",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::stubs::StubMethod {
        owner: "Args",
        name: "get",
        doc: "The node behind `name`, or `default` when there is none.",
        params: &[
            dex_core::stubs::StubField { name: "name", ty: "String" },
            dex_core::stubs::StubField { name: "default", ty: "Option<NodeUid>" },
        ],
        returns: "Option<NodeUid>",
        is_static: false,
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
    if let Some(n) = any.downcast_ref::<Integer>() {
        return Some(ScriptValue::Int(n.value));
    }
    if let Some(n) = any.downcast_ref::<Float>() {
        return Some(ScriptValue::Float(n.value));
    }
    if let Some(b) = any.downcast_ref::<Bool>() {
        return Some(ScriptValue::Bool(b.value));
    }
    if let Some(t) = any.downcast_ref::<Table>() {
        return Some(ScriptValue::Table(t.batch().clone()));
    }
    // TODO: GENERALIZE
    if let Some(scroll) = any.downcast_ref::<ScrollLayout>()
        && let LayoutChild::Node(inner) = &scroll.child
    {
        return node_to_value(&**inner);
    }
    None
}

dex_dynamic::__rt::inventory::submit! {
    dex_core::scripting::NodeCoercion(to_dyn_node_py)
}

/// The one canonical mapping from a Python value to a node.
pub fn to_dyn_node_py(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Arc<dyn Node> {
    use pyo3::prelude::*;
    if obj.is_none() {
        return Arc::new(Nothing);
    }
    // `bool` before `int`: in Python `bool` extracts as `int` too.
    if let Ok(v) = obj.extract::<bool>() {
        return Arc::new(Bool::new(v));
    }
    if let Ok(v) = obj.extract::<i64>() {
        return Arc::new(Integer::new(v));
    }
    if let Ok(v) = obj.extract::<f64>() {
        return Arc::new(Float::new(v));
    }
    if let Ok(v) = obj.extract::<String>() {
        return Arc::new(Label::new(v));
    }
    // A node type of our own, before anything that has to be *tried* rather
    // than tested: these are the values a drawing script hands over thousands
    // of times a frame, and they are recognised by a type check.
    for extractor in dex_dynamic::__rt::inventory::iter::<NodeExtractor> {
        if let Some(node) = (extractor.from_python)(obj) {
            return node;
        }
    }
    // Columnar data is a table.
    if let Some(batch) = looks_like_arrow(obj)
        .then(|| record_batch_from_python(obj))
        .flatten()
    {
        return Arc::new(Table::new(ArrowData(batch)));
    }
    Arc::new(DynamicNode::from_python(obj))
}

/**
    Whether `obj` could be Arrow data at all.

    [`record_batch_from_python`] finds out by *trying*, and on anything else
    that costs a pyarrow import and a raised Python exception — tens of
    microseconds each time. Once, that is nothing. For every shape a drawing
    script hands over, every frame, it was the whole frame: seven milliseconds
    of a nine-millisecond plot, spent failing to read a polygon as a table.

    So the attempt is gated on the object carrying one of the names Arrow data
    is recognised by — the C data interface either side of it, or the chunked
    methods the fallback path calls.
*/
fn looks_like_arrow(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> bool {
    use pyo3::prelude::*;
    const MARKERS: [&str; 4] = [
        "__arrow_c_array__",
        "__arrow_c_stream__",
        "combine_chunks",
        "to_batches",
    ];
    MARKERS
        .iter()
        .any(|marker| obj.hasattr(*marker).unwrap_or(false))
}

/// A `RecordBatch` from anything holding Arrow columns, or `None`.
fn record_batch_from_python(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Option<RecordBatch> {
    use arrow::pyarrow::FromPyArrow;
    use pyo3::prelude::*;

    if let Ok(batch) = RecordBatch::from_pyarrow_bound(obj) {
        return Some(batch);
    }
    // Chunked: one batch per chunk until they are combined.
    let combined = obj.call_method0("combine_chunks").ok()?;
    let batches = combined.call_method0("to_batches").ok()?;
    match batches.len().ok()? {
        0 => {
            let schema =
                arrow::datatypes::Schema::from_pyarrow_bound(&obj.getattr("schema").ok()?).ok()?;
            Some(RecordBatch::new_empty(Arc::new(schema)))
        }
        _ => RecordBatch::from_pyarrow_bound(&batches.get_item(0).ok()?).ok(),
    }
}

/// The result of asking a dynamic object to draw itself.
pub enum DynDraw {
    /// The object drew; here is what it reported, exactly as a Rust node would.
    Drawn(DrawResult),
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
        // The handle is invalidated before `enter` returns, so `ctx` is free
        // to use again below without any live alias to it.
        let entered = PyDrawContext::enter(py, ctx, |pyctx| {
            bound.call_method1("draw", (pyctx.clone(),))
        });
        let result = match entered {
            Ok(v) => v,
            Err(e) => return DynDraw::Error(e.to_string()),
        };
        match result {
            Err(e) => DynDraw::Error(e.to_string()),
            Ok(ret) if ret.is_none() => DynDraw::Drawn(DrawResult::Complete { region: None }),
            // A `DrawResult` is the node's own report; anything else is a value
            // to draw, so check for it before coercing.
            Ok(ret) => match ret.extract::<DrawResult>() {
                Ok(reported) => DynDraw::Drawn(reported),
                Err(_) => {
                    let arc = to_dyn_node_py(&ret);
                    let constraints = ctx.constraints;
                    DynDraw::Drawn(ctx.draw_node(&*arc, constraints))
                }
            },
        }
    })
}

/// A failure while running a script.
#[derive(Debug)]
pub enum ScriptError {
    /// The source contained an interior NUL byte (Python only).
    InteriorNul,
    Python(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InteriorNul => write!(f, "script source contains an interior NUL byte"),
            Self::Python(e) => write!(f, "python error: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<NulError> for ScriptError {
    fn from(_: NulError) -> Self {
        Self::InteriorNul
    }
}

// ================================================================================
// THE SCRIPT ENVIRONMENT
// ================================================================================

/// Every name the prelude binds at the top level, read from its source.
///
/// Used to decide whether an expression needs the prelude run before it can be
/// evaluated. Reading the source rather than the namespace is the whole point:
/// the namespace only exists once the prelude has run, which is the cost this
/// is trying to avoid paying.
///
/// It sees `def`, `class`, and plain assignment at column zero, which is how
/// the prelude defines everything. A name bound some other way — inside a
/// top-level loop, or through `globals()` — would be missed, and an expression
/// naming it would be evaluated without the prelude and raise `NameError`.
fn prelude_top_level_names(prelude: &str) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for line in prelude.lines() {
        // Column zero only: anything indented is inside something else.
        if line.starts_with([' ', '\t']) || line.is_empty() {
            continue;
        }
        let name = if let Some(rest) = line.strip_prefix("def ") {
            rest.split('(').next()
        } else if let Some(rest) = line.strip_prefix("class ") {
            rest.split(['(', ':']).next()
        } else {
            // An assignment, but not a comparison or an augmented one: the name
            // is everything before a lone `=`, and it has to be an identifier.
            line.split_once('=')
                .filter(|(before, after)| {
                    !before.ends_with(['=', '!', '<', '>', '+', '-', '*', '/'])
                        && !after.starts_with('=')
                })
                .map(|(before, _)| before.split(':').next().unwrap_or(before))
        };
        if let Some(name) = name.map(str::trim)
            && is_valid_ident(name)
        {
            names.insert(name.to_owned());
        }
    }
    names
}

/**
    The Python namespace one run happens in: `dex`, the prelude, the arguments.

    Built once and used more than once. A lambda checks its arguments before it
    hands them to its script, and both halves want the same namespace — so
    preparing it twice, which is what running the prelude twice amounts to, was
    most of what a run cost.

    The prelude is loaded **lazily** and at most once. A declaration that names
    nothing the prelude defines is settled without it, and a declaration that
    refuses ends the run before any script needs it — so a lambda sitting on a
    guard that says "stop" costs no prelude at all.

    Precedence, lowest first: the prelude's exports, then the arguments, then
    whatever the script itself defines. Each is more local than the last, and
    the prelude keeps its own module namespace, so shadowing one of its names
    changes what the *script* sees without reaching inside the prelude's own
    functions.
*/
pub struct ScriptEnv<'py> {
    py: pyo3::Python<'py>,
    dex: pyo3::Bound<'py, pyo3::types::PyModule>,
    globals: pyo3::Bound<'py, pyo3::types::PyDict>,
    prelude: String,
    defines: std::collections::HashSet<String>,
    /// Bound once the prelude has run. Held because the functions exported out
    /// of it resolve their own globals *there*, not in the script's namespace.
    loaded: Option<pyo3::Bound<'py, pyo3::types::PyModule>>,
}

impl<'py> ScriptEnv<'py> {
    /// A namespace with `dex` in it and nothing else yet.
    pub fn new(py: pyo3::Python<'py>, prelude: &str) -> pyo3::PyResult<Self> {
        use pyo3::prelude::*;
        use pyo3::types::PyDict;

        let dex = dex_dynamic::build_python_module(py)?;
        let globals = PyDict::new(py);
        // Present the exec namespace as the `__main__` module.
        globals.set_item("__name__", "__main__")?;
        globals.set_item("dex", &dex)?;
        Ok(Self {
            py,
            dex,
            globals,
            prelude: prelude.to_owned(),
            defines: prelude_top_level_names(prelude),
            loaded: None,
        })
    }

    /// Seed `dex.ws` (writes) and `dex.snapshot` (reads).
    pub fn with_runtime(
        self,
        handle: &WorkspaceActionHandle,
        graph: GraphSnapshot,
    ) -> pyo3::PyResult<Self> {
        use pyo3::prelude::*;

        let ws = pyo3::Bound::new(self.py, handle.clone())?;
        self.dex.add("ws", ws)?;
        let snapshot = pyo3::Bound::new(self.py, dex_core::snapshot::PySnapshot::new(graph))?;
        self.dex.add("snapshot", snapshot)?;
        Ok(self)
    }

    /// Seed each argument as a global, and `dex.args` with where it came from.
    pub fn with_args(self, args: &[ScriptArg]) -> pyo3::PyResult<Self> {
        use pyo3::prelude::*;

        let arg_table = pyo3::Bound::new(self.py, ScriptArgs::new(args))?;
        self.dex.add("args", arg_table)?;
        seed_globals(self.py, &self.globals, args)?;
        Ok(self)
    }

    /// The namespace a script or an expression runs in.
    pub fn globals(&self) -> &pyo3::Bound<'py, pyo3::types::PyDict> {
        &self.globals
    }

    /// The interpreter this namespace belongs to.
    pub fn python(&self) -> pyo3::Python<'py> {
        self.py
    }

    /// Whether `expr` names anything only the prelude defines.
    pub fn expression_needs_prelude(&self, expr: &str) -> bool {
        use pyo3::prelude::*;

        if self.loaded.is_some() || self.defines.is_empty() {
            return false;
        }
        let Ok(ast) = self.py.import("ast") else {
            return true;
        };
        // A parse that fails says nothing about what the expression names, so
        // the prelude is run and the eval is left to report the syntax error.
        let Ok(tree) = ast.call_method1("parse", (expr, "<declaration>", "eval")) else {
            return true;
        };
        let (Ok(walk), Ok(name_ty)) = (ast.call_method1("walk", (tree,)), ast.getattr("Name"))
        else {
            return true;
        };
        let Ok(nodes) = walk.try_iter() else {
            return true;
        };
        for node in nodes.flatten() {
            let Ok(true) = node.is_instance(&name_ty) else {
                continue;
            };
            let Ok(id) = node.getattr("id").and_then(|id| id.extract::<String>()) else {
                return true;
            };
            let bound = self.globals.contains(&id).unwrap_or(false);
            if !bound && self.defines.contains(&id) {
                return true;
            }
        }
        false
    }

    /**
        Run the prelude, once, into a module namespace of its own.

        Two things come of the separate namespace. `dex.prelude` is that module,
        so every name the library defines can be reached unambiguously however
        the script's own namespace is arranged. And the library's functions
        resolve each other *there* — so a script, or an argument, named `line`
        shadows the export without breaking the `card` that calls it.

        The public names are copied out into the script's namespace as well, so
        `line(...)` keeps working; a name already bound wins, which is what puts
        the arguments above the prelude.
    */
    pub fn ensure_prelude(&mut self) -> pyo3::PyResult<()> {
        use pyo3::prelude::*;
        use pyo3::types::PyModule;

        if self.loaded.is_some() {
            return Ok(());
        }
        let module = PyModule::new(self.py, "dex_prelude")?;
        let namespace = module.dict();
        namespace.set_item("dex", &self.dex)?;
        let code = CString::new(self.prelude.as_str())
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("prelude contains a NUL byte"))?;
        self.py
            .run(code.as_c_str(), Some(&namespace), Some(&namespace))?;

        // Reachable as a whole, and exported name by name.
        self.dex.add("prelude", &module)?;
        for (key, value) in namespace.iter() {
            let Ok(name) = key.extract::<String>() else {
                continue;
            };
            if name.starts_with('_') || self.globals.contains(&name).unwrap_or(false) {
                continue;
            }
            self.globals.set_item(name, value)?;
        }
        self.loaded = Some(module);
        Ok(())
    }

    /// Run `source` in this namespace and call the `transform` it defines.
    pub fn run(&mut self, source: &str) -> Result<ScriptOutput, ScriptError> {
        use pyo3::prelude::*;

        let map_err = |e: pyo3::PyErr| ScriptError::Python(e.to_string());
        let code = CString::new(source)?;
        self.ensure_prelude().map_err(map_err)?;
        self.py
            .run(code.as_c_str(), Some(&self.globals), Some(&self.globals))
            .map_err(map_err)?;
        let Some(transform) = self.globals.get_item("transform").map_err(map_err)? else {
            return Err(ScriptError::Python(
                "script must define a `transform` function".to_owned(),
            ));
        };
        let result = transform.call0().map_err(map_err)?;
        Ok(extract_python(&result))
    }
}

/**
    Run `source` as Python with `handle` as context and `args` seeded as globals.

    For a caller holding values and nothing else. A lambda, which knows what
    node each value came from, wants [`run_script_with`] instead — `dex.args` is
    empty here, since there is nothing to put in it.
*/
pub fn run_script(
    source: &str,
    py_prelude: &str,
    handle: &WorkspaceActionHandle,
    args: &[(String, ScriptValue)],
    graph: GraphSnapshot,
) -> Result<ScriptOutput, ScriptError> {
    let args: Vec<ScriptArg> = args
        .iter()
        .map(|(name, value)| ScriptArg::detached(name.clone(), value.clone()))
        .collect();
    run_script_with(source, py_prelude, handle, &args, graph)
}

/// Run `source` with arguments that know where they came from, so `dex.args`
/// can name a node for each one.
pub fn run_script_with(
    source: &str,
    py_prelude: &str,
    handle: &WorkspaceActionHandle,
    args: &[ScriptArg],
    graph: GraphSnapshot,
) -> Result<ScriptOutput, ScriptError> {
    pyo3::Python::attach(|py| {
        let map_err = |e: pyo3::PyErr| ScriptError::Python(e.to_string());
        let mut env = ScriptEnv::new(py, py_prelude)
            .and_then(|env| env.with_runtime(handle, graph))
            .and_then(|env| env.with_args(args))
            .map_err(map_err)?;
        env.run(source)
    })
}

/**
    Seed each argument as a global, as the script will see it.

    Shared with the argument-type checks, so what a check looks at is the same
    object the script is about to be handed.
*/
pub fn seed_globals(
    py: pyo3::Python<'_>,
    globals: &pyo3::Bound<'_, pyo3::types::PyDict>,
    args: &[ScriptArg],
) -> pyo3::PyResult<()> {
    use pyo3::prelude::*;
    for ScriptArg { name, value, .. } in args {
        match value {
            ScriptValue::Str(s) => globals.set_item(name, s),
            ScriptValue::Int(i) => globals.set_item(name, *i),
            ScriptValue::Float(f) => globals.set_item(name, *f),
            ScriptValue::Bool(b) => globals.set_item(name, *b),
            ScriptValue::Node(uid) => {
                let handle = Bound::new(py, NodeHandle(*uid))?;
                globals.set_item(name, handle)
            }
            ScriptValue::Table(rb) => {
                use arrow::pyarrow::ToPyArrow;
                let obj = rb.to_pyarrow(py)?;
                globals.set_item(name, obj)
            }
            ScriptValue::Nothing => globals.set_item(name, ()),
        }?;
    }
    Ok(())
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
