use std::ffi::CString;

use pyo3::PyErr;

pub mod hash;
pub mod python;

pub(crate) type TransformResult<T> = Result<T, TransformErr>;

#[derive(Debug)]
pub enum TransformErr {
    MissingTransformFunction,
    PyErr(PyErr),
    OutputNotUnderstood,
}

impl From<PyErr> for TransformErr {
    fn from(e: PyErr) -> Self {
        Self::PyErr(e)
    }
}

trait ToCString {
    fn to_c_string(self) -> CString;
}

impl<S: AsRef<str>> ToCString for S {
    fn to_c_string(self) -> CString {
        fn inner(s: &str) -> CString {
            CString::new(s.replace('\0', ""))
                .expect("CString construction should not fail; internal nulls have been stripped")
        }

        inner(self.as_ref())
    }
}
