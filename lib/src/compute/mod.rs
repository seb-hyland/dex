use std::{ffi::CString, fmt::Display};

use arrow::array::RecordBatch;
use pyo3::PyErr;

pub mod hash;
pub mod python;

pub struct TransformArgument {
    pub name: String,
    pub value: TransformValue,
}

#[derive(Debug)]
pub enum TransformValue {
    Dataframe(RecordBatch),
    String(String),
    Int(i32),
    Float(f64),
}

pub(crate) type TransformResult<T> = Result<T, TransformErr>;

#[derive(Debug)]
pub enum TransformErr {
    MissingTransformFunction,
    OutputNotUnderstood,
    PyErr(PyErr),
}

impl Display for TransformErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTransformFunction => write!(f, "Could not find a transform function"),
            Self::OutputNotUnderstood => {
                write!(f, "Output value could not be converted into a node")
            }
            Self::PyErr(e) => write!(f, "Python error:\n{e}"),
        }
    }
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
