use arrow::{
    array::RecordBatch,
    pyarrow::{FromPyArrow, ToPyArrow},
};
use pyo3::{
    Bound, PyAny, PyErr, PyResult, Python,
    exceptions::PyTypeError,
    types::{
        PyAnyMethods, PyFloat, PyInt, PyList, PyListMethods, PyModule, PyModuleMethods, PyString,
        PyTuple, PyTypeMethods,
    },
};

use crate::compute::{ToCString, TransformArgument, TransformErr, TransformResult, TransformValue};

impl TransformValue {
    fn into_pyany(self, py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
        match self {
            Self::Dataframe(df) => df.to_pyarrow(py),
            Self::String(s) => Ok(PyString::new(py, &s).into_any()),
            Self::Int(i) => Ok(PyInt::new(py, i).into_any()),
            Self::Float(f) => Ok(PyFloat::new(py, f).into_any()),
        }
    }
}

pub fn apply_transform(
    code: &str,
    args: Vec<TransformArgument>,
    venv: Option<&str>,
) -> TransformResult<TransformValue> {
    Python::attach(|py| -> TransformResult<TransformValue> {
        let code = code.to_c_string();
        if let Some(venv_path) = venv {
            let path: Bound<'_, PyList> = py
                .import("sys")?
                .getattr("path")?
                .extract()
                .expect("Path should always be a PyList");
            path.insert(0, venv_path)?;
        }

        let transform_mod =
            PyModule::from_code(py, &code, c"transform.py", c"TransformationModule")?;

        let globals = transform_mod.dict();
        for arg in args {
            let name = PyString::new(py, &arg.name);
            globals.set_item(name, arg.value.into_pyany(py)?)?;
        }

        let transform_func = transform_mod
            .getattr_opt("transform")
            .map(|found_function| found_function.ok_or(TransformErr::MissingTransformFunction))??;

        let result = transform_func.call0()?;

        if let Ok(i) = result.extract::<i32>() {
            Ok(TransformValue::Int(i))
        } else if let Ok(f) = result.extract::<f64>() {
            Ok(TransformValue::Float(f))
        } else if let Ok(str) = result.extract::<String>() {
            Ok(TransformValue::String(str))
        } else {
            let arrow_result = match result
                .get_type()
                .fully_qualified_name()
                .map(|s| s.to_string())
                .as_deref()
            {
                Ok("pandas.DataFrame") => {
                    let pa = py.import("pyarrow")?;
                    let table = pa
                        .getattr("Table")?
                        .call_method1("from_pandas", (result,))?;

                    table
                        .call_method0("combine_chunks")
                        .and_then(|table| table.call_method0("to_batches"))
                        .and_then(|list| {
                            list.extract::<Bound<'_, PyList>>().map_err(|_| {
                                PyTypeError::new_err(
                                    "Expected Table to RecordBatch conversion to yield a list",
                                )
                            })
                        })
                        .and_then(|list| list.get_item(0))
                        .map_err(TransformErr::from)
                }
                Ok("pandas.Series") => {
                    let result = result.call_method0("to_frame")?;

                    let pa = py.import("pyarrow")?;
                    let table = pa
                        .getattr("Table")?
                        .call_method1("from_pandas", (result,))?;

                    table
                        .call_method0("combine_chunks")
                        .and_then(|table| table.call_method0("to_batches"))
                        .and_then(|list| {
                            list.extract::<Bound<'_, PyList>>().map_err(|_| {
                                PyTypeError::new_err(
                                    "Expected Table to RecordBatch conversion to yield a list",
                                )
                            })
                        })
                        .and_then(|list| list.get_item(0))
                        .map_err(TransformErr::from)
                }
                Ok("polars.dataframe.frame.DataFrame") => result
                    .call_method0("to_arrow")
                    .and_then(|table| table.call_method0("combine_chunks"))
                    .and_then(|table| table.call_method0("to_batches"))
                    .and_then(|list| {
                        list.extract::<Bound<'_, PyList>>().map_err(|_| {
                            PyTypeError::new_err(
                                "Expected Table to RecordBatch conversion to yield a list",
                            )
                        })
                    })
                    .and_then(|list| list.get_item(0))
                    .map_err(TransformErr::from),
                Ok("pyarrow.lib.Table") => result
                    .call_method0("combine_chunks")
                    .and_then(|table| table.call_method0("to_batches"))
                    .and_then(|list| {
                        list.extract::<Bound<'_, PyList>>().map_err(|_| {
                            PyTypeError::new_err(
                                "Expected Table to RecordBatch conversion to yield a list",
                            )
                        })
                    })
                    .and_then(|list| list.get_item(0))
                    .map_err(TransformErr::from),
                Ok("pyarrow.lib.RecordBatch") => Ok(result),
                other => {
                    println!("{:?}", other);
                    Err(TransformErr::OutputNotUnderstood)
                }
            }?;
            let batch = RecordBatch::from_pyarrow_bound(&arrow_result)?;
            Ok(TransformValue::Dataframe(batch))
        }
    })
}
