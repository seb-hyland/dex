use std::cell::Cell;
use std::sync::Arc;

use pyo3::prelude::*;

use crate::{Node, NodeUid};

/// A fully type-erased id, as seen by scripts. Replaces [`NodeUid`]
#[utils::dynamic_type(name = "NodeUid", no_copy)]
#[utils::portable]
pub struct NodeHandle(pub NodeUid);

#[pyo3::pymethods]
impl NodeHandle {
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<NodeHandle>().is_ok_and(|o| o.0 == self.0)
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    /**
        Rewrite this handle when it is copied as part of a deep clone.

        `copy.deepcopy` reaches every handle in a script's object graph — nested
        in lists, dicts, tuples, other objects, cycles included — so hooking the
        leaf is all it takes. The clone's id map rides along in `memo` under
        [`CLONE_REMAP_KEY`], which is where `deepcopy` already threads per-copy
        context.
    */
    #[pyo3(signature = (memo=None))]
    fn __deepcopy__(&self, memo: Option<Bound<'_, PyAny>>) -> NodeHandle {
        let replacement = memo
            .and_then(|memo| memo.get_item(CLONE_REMAP_KEY).ok())
            .and_then(|table| table.get_item(NodeHandle(self.0)).ok())
            .and_then(|found| found.extract::<NodeHandle>().ok());
        NodeHandle(replacement.map_or(self.0, |handle| handle.0))
    }

    fn __copy__(&self) -> NodeHandle {
        // A plain copy is not part of a clone, so the id stands.
        NodeHandle(self.0)
    }
}

/**
    The `copy.deepcopy` memo key carrying a clone's id map.

    `deepcopy` keys its own bookkeeping by `id()`, so a string key rides
    alongside it untouched.
*/
pub const CLONE_REMAP_KEY: &str = "__dex_clone_remap__";

/**
    A `deepcopy` memo seeded with `map`, so handles buried in a script's state
    rewrite themselves through `NodeHandle`'s `__deepcopy__`.

    Handed to `copy.deepcopy` as its second argument. Keyed by handle, which
    hashes and compares by uid.
*/
pub fn clone_memo<'py>(
    py: Python<'py>,
    map: &std::collections::HashMap<NodeUid, NodeUid>,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    use pyo3::types::PyDictMethods;

    let table = pyo3::types::PyDict::new(py);
    for (from, to) in map {
        table.set_item(NodeHandle(*from), NodeHandle(*to))?;
    }
    let memo = pyo3::types::PyDict::new(py);
    memo.set_item(CLONE_REMAP_KEY, table)?;
    Ok(memo)
}

/// Coerces a Python value into some type of node (Rust or dynamic-defined).
pub struct NodeExtractor {
    pub from_python: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> Option<Arc<dyn Node>>,
}

dex_dynamic::__rt::inventory::collect!(NodeExtractor);

/// Any Python value can become some node.
pub struct NodeCoercion(pub fn(&Bound<'_, PyAny>) -> Arc<dyn Node>);

dex_dynamic::__rt::inventory::collect!(NodeCoercion);

/// Apply the registered [`NodeCoercion`], if one has been contributed.
pub fn coerce_to_node(obj: &Bound<'_, PyAny>) -> PyResult<Arc<dyn Node>> {
    dex_dynamic::__rt::inventory::iter::<NodeCoercion>
        .into_iter()
        .next()
        .map(|c| (c.0)(obj))
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("no node coercion has been registered")
        })
}

// ======================================================================
// The Python value boundary
// ======================================================================

/// A type reconstructible from a Python value.
/// Implemented automatically for every `#[dynamic_type]`.
pub trait FromDynamic: Sized {
    fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self>;
}

/// A type handed back to Python.
pub trait IntoDynamic {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>>;
}

/// Implement both directions for types pyo3 already round-trips natively.
#[macro_export]
macro_rules! impl_dynamic_native {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::scripting::FromDynamic for $ty {
            fn from_dynamic(
                obj: &::pyo3::Bound<'_, ::pyo3::PyAny>,
            ) -> ::pyo3::PyResult<Self> {
                use ::pyo3::prelude::*;
                obj.extract::<$ty>()
            }
        }

        impl $crate::scripting::IntoDynamic for $ty {
            fn into_dynamic(
                self,
                py: ::pyo3::Python<'_>,
            ) -> ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>> {
                ::pyo3::IntoPyObjectExt::into_py_any(self, py)
            }
        }
    )*};
}

impl_dynamic_native!(
    bool, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, char, String,
);

impl FromDynamic for () {
    fn from_dynamic(_obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(())
    }
}

impl IntoDynamic for () {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(py.None())
    }
}

impl<T: FromDynamic> FromDynamic for Option<T> {
    fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if obj.is_none() {
            return Ok(None);
        }
        T::from_dynamic(obj).map(Some)
    }
}

impl<T: IntoDynamic> IntoDynamic for Option<T> {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Some(v) => v.into_dynamic(py),
            None => Ok(py.None()),
        }
    }
}

impl<T: FromDynamic> FromDynamic for Vec<T> {
    fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        obj.try_iter()?
            .map(|item| T::from_dynamic(&item?))
            .collect()
    }
}

impl<T: IntoDynamic> IntoDynamic for Vec<T> {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let items = self
            .into_iter()
            .map(|v| v.into_dynamic(py))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(pyo3::types::PyList::new(py, items)?.into_any().unbind())
    }
}

/// Tuples cross as Python tuples, so multi-valued responses need no wrapper type.
macro_rules! impl_dynamic_tuple {
    ($($name:ident),+) => {
        #[allow(non_snake_case)]
        impl<$($name: FromDynamic),+> FromDynamic for ($($name,)+) {
            fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
                let mut idx = 0usize;
                $(
                    let $name = $name::from_dynamic(&obj.get_item(idx)?)?;
                    idx += 1;
                )+
                let _ = idx;
                Ok(($($name,)+))
            }
        }

        #[allow(non_snake_case)]
        impl<$($name: IntoDynamic),+> IntoDynamic for ($($name,)+) {
            fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
                let ($($name,)+) = self;
                let items = vec![$($name.into_dynamic(py)?),+];
                Ok(pyo3::types::PyTuple::new(py, items)?.into_any().unbind())
            }
        }
    };
}

impl_dynamic_tuple!(A, B);
impl_dynamic_tuple!(A, B, C);
impl_dynamic_tuple!(A, B, C, D);

/// A node id crosses the boundary in its erased form.
impl<T: ?Sized> FromDynamic for NodeUid<T> {
    fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(obj.extract::<NodeHandle>()?.0.cast())
    }
}

impl<T: ?Sized> IntoDynamic for NodeUid<T> {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, NodeHandle(self.erase()))?.into_any())
    }
}

/// An opaque handle to a live node value handed back to a script.
#[pyo3::pyclass(from_py_object, name = "Node")]
#[derive(Clone)]
pub struct DynNode(pub Arc<dyn Node>);

dex_dynamic::__rt::inventory::submit! {
    dex_dynamic::DynamicBinding {
        name: "Node",
        register_python: |m| {
            use pyo3::types::PyModuleMethods;
            m.add_class::<DynNode>()
        },
    }
}

dex_dynamic::__rt::inventory::submit! {
    crate::stubs::StubClass {
        name: "Node",
        doc: "An opaque handle to a node value.",
        fields: &[],
        constructible: false,
        variants: &[],
    }
}

/// Any Python value can stand in for a node; one handed back by a previous
/// call round-trips as itself rather than being re-coerced.
impl FromDynamic for Arc<dyn Node> {
    fn from_dynamic(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(node) = obj.extract::<DynNode>() {
            return Ok(node.0);
        }
        coerce_to_node(obj)
    }
}

impl IntoDynamic for Arc<dyn Node> {
    fn into_dynamic(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, DynNode(self))?.into_any())
    }
}

// ======================================================================
// Pickling
// ======================================================================

/**
    Reconstructs a bound value from the bytes its `__reduce__` captured.

    A `#[pyclass]` keeps its data in Rust rather than a Python `__dict__`, so
    the default pickle protocol cannot describe how to rebuild one and raises
    `cannot pickle`. Each bound type therefore reduces to `dex._rebuild(name,
    bytes)`, and registers here how to turn those bytes back into a value.
*/
pub struct DynamicRebuild {
    pub name: &'static str,
    pub from_bytes: fn(&[u8], Python<'_>) -> PyResult<Py<PyAny>>,
}

dex_dynamic::__rt::inventory::collect!(DynamicRebuild);

dex_dynamic::__rt::inventory::submit! {
    dex_dynamic::DynamicBinding {
        name: "_rebuild",
        register_python: |m| {
            use pyo3::types::PyModuleMethods;
            m.add_function(pyo3::wrap_pyfunction!(rebuild, m)?)
        },
    }
}

/**
    Capture a bound value as bytes for its `__reduce__`.
    CBOR (RFC 8949): binary and compact.
*/
pub fn reduce_to_bytes<T: serde::Serialize>(value: &T) -> PyResult<Vec<u8>> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("could not capture value: {e}"))
    })?;
    Ok(out)
}

/// The inverse of [`reduce_to_bytes`].
pub fn reduce_from_bytes<T: serde::de::DeserializeOwned>(data: &[u8]) -> PyResult<T> {
    ciborium::from_reader(data).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("could not restore value: {e}"))
    })
}

/// Rebuild a bound value from its captured bytes. Referenced by every
/// generated `__reduce__`, which is why the `dex` module must be importable.
#[pyfunction]
#[pyo3(name = "_rebuild")]
pub fn rebuild(py: Python<'_>, name: &str, data: Vec<u8>) -> PyResult<Py<PyAny>> {
    dex_dynamic::__rt::inventory::iter::<DynamicRebuild>
        .into_iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "cannot restore `{name}`: no such bound type is registered"
            ))
        })
        .and_then(|entry| (entry.from_bytes)(&data, py))
}

// ======================================================================
// Scoped borrows
// ======================================================================

/// A script-facing handle to a value borrowed from the Rust stack.
pub struct Scoped<T> {
    ptr: Cell<*mut T>,
}

impl<T> Scoped<T> {
    /// A handle over `value`, live until [`Scoped::invalidate`].
    ///
    /// # Safety
    /// The caller must invalidate this handle before `value`'s borrow ends.
    /// Prefer [`Scoped::enter`], which does so via a guard.
    pub unsafe fn new(value: &mut T) -> Self {
        Self {
            ptr: Cell::new(value as *mut T),
        }
    }

    /// Run `f` against the borrowed value, or return [`None`] if the handle has expired.
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let ptr = self.ptr.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: the pointer targets the stack value of the in-progress call,
        // and is nulled the moment that call returns.
        Some(f(unsafe { &mut *ptr }))
    }

    /// Expire this handle. Every later [`Scoped::with`] is a no-op.
    pub fn invalidate(&self) {
        self.ptr.set(std::ptr::null_mut());
    }

    pub fn is_live(&self) -> bool {
        !self.ptr.get().is_null()
    }
}

/// The shared-borrow counterpart of [`Scoped`].
pub struct ScopedRef<T> {
    ptr: Cell<*const T>,
}

impl<T> ScopedRef<T> {
    /// A handle over `value`, live until [`ScopedRef::invalidate`].
    ///
    /// # Safety
    /// The caller must invalidate this handle before `value`'s borrow ends.
    pub unsafe fn new(value: &T) -> Self {
        Self {
            ptr: Cell::new(value as *const T),
        }
    }

    /// Run `f` against the borrowed value, or return [`None`] if the handle has expired.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let ptr = self.ptr.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: the pointer targets a value borrowed for the in-progress
        // call, and is nulled the moment that call returns. Only `&T` is
        // ever produced from it.
        Some(f(unsafe { &*ptr }))
    }

    /// Expire this handle. Every later [`ScopedRef::with`] is a no-op.
    pub fn invalidate(&self) {
        self.ptr.set(std::ptr::null());
    }

    pub fn is_live(&self) -> bool {
        !self.ptr.get().is_null()
    }
}

/// The error a script sees when it uses a handle outside the call it belongs to.
pub fn expired_handle() -> pyo3::PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "this context handle is no longer valid: it belongs to a call that has already returned",
    )
}

/// Invalidates its handle on drop, so an early return cannot leave one live.
pub struct ScopeGuard<'a, T>(&'a Scoped<T>);

impl<T> Drop for ScopeGuard<'_, T> {
    fn drop(&mut self) {
        self.0.invalidate();
    }
}

impl<T> Scoped<T> {
    /// Guard `self` so it is invalidated when the guard leaves scope.
    pub fn guard(&self) -> ScopeGuard<'_, T> {
        ScopeGuard(self)
    }
}
