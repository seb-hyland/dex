use arrow::{
    array::RecordBatch,
    pyarrow::{FromPyArrow, ToPyArrow},
};
use pyo3::{
    Bound, PyErr, Python,
    exceptions::PyTypeError,
    types::{PyAnyMethods, PyList, PyListMethods, PyModule, PyTuple, PyTypeMethods},
};

use crate::compute::{ToCString, TransformErr, TransformResult};

pub fn apply_transform(
    inputs: Vec<&RecordBatch>,
    code: &str,
    venv: Option<&str>,
) -> TransformResult<RecordBatch> {
    Python::attach(|py| -> TransformResult<RecordBatch> {
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
        let transform_func = transform_mod
            .getattr_opt("transform")
            .map(|found_function| found_function.ok_or(TransformErr::MissingTransformFunction))??;

        let py_inputs: Vec<_> = inputs
            .into_iter()
            .map(|batch| batch.to_pyarrow(py))
            .collect::<Result<Vec<_>, PyErr>>()?;
        let inputs_tuple = PyTuple::new(py, py_inputs)?;

        let result = transform_func.call1(inputs_tuple)?;
        let res_ty = result.get_type();
        println!("{:?}", res_ty.fully_qualified_name());
        let arrow_result = match res_ty
            .fully_qualified_name()
            .map(|s| s.to_string())
            .as_deref()
        {
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
            _ => Err(TransformErr::OutputNotUnderstood),
        }?;
        let batch = RecordBatch::from_pyarrow_bound(&arrow_result)?;
        Ok(batch)
    })
}
