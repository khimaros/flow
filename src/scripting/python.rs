use super::*;
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyList, PyModule};
use tracing::{debug, info};

#[pyfunction]
fn log(msg: String) {
    tracing::info!("[python script] {}", msg);
}

pub struct PythonEngine;

impl Default for PythonEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonEngine {
    pub fn new() -> Self {
        Self
    }

    /// set up Python environment and execute a script, returning the globals dict
    fn setup_and_run_script<'py>(
        py: Python<'py>,
        script: &str,
        module: &Bound<'py, PyModule>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let globals = module.dict();
        globals.set_item("log", wrap_pyfunction!(log, module)?)?;

        // monkey-patch signal.signal to be a no-op - signals only work in main thread
        // but user scripts may import libraries (like meshtastic) that try to use them
        py.run(
            c"import signal; signal.signal = lambda *args, **kwargs: signal.SIG_DFL",
            Some(&globals),
            Some(&globals),
        )?;

        // execute the script in the module namespace
        py.run(
            &std::ffi::CString::new(script).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid script: {}", e))
            })?,
            Some(&globals),
            Some(&globals),
        )?;

        Ok(globals)
    }

    /// set up Python environment with context for execution (cancellation + progress)
    fn setup_and_run_script_with_context<'py>(
        py: Python<'py>,
        script: &str,
        module: &Bound<'py, PyModule>,
        ctx: Arc<ScriptContext>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let globals = module.dict();
        globals.set_item("log", wrap_pyfunction!(log, module)?)?;

        // monkey-patch signal.signal to be a no-op
        py.run(
            c"import signal; signal.signal = lambda *args, **kwargs: signal.SIG_DFL",
            Some(&globals),
            Some(&globals),
        )?;

        // create is_cancelled function
        let cancelled_flag = ctx.cancelled.clone();
        let is_cancelled_fn = PyCFunction::new_closure(
            py,
            Some(c"is_cancelled"),
            Some(c"check if execution has been cancelled"),
            move |_args: &Bound<'_, pyo3::types::PyTuple>,
                  _kwargs: Option<&Bound<'_, PyDict>>|
                  -> PyResult<bool> { Ok(cancelled_flag.load(Ordering::Relaxed)) },
        )?;
        globals.set_item("is_cancelled", is_cancelled_fn)?;

        // create report_progress function
        let progress_fn = ctx.report_progress.clone();
        let report_progress_fn = PyCFunction::new_closure(
            py,
            Some(c"report_progress"),
            Some(c"Report progress (0.0 to 1.0) with optional message"),
            move |args: &Bound<'_, pyo3::types::PyTuple>,
                  _kwargs: Option<&Bound<'_, PyDict>>|
                  -> PyResult<()> {
                let progress: f64 = args.get_item(0)?.extract()?;
                let message: Option<String> = if args.len() > 1 {
                    args.get_item(1)?.extract().ok()
                } else {
                    None
                };
                progress_fn(progress as f32, message);
                Ok(())
            },
        )?;
        globals.set_item("report_progress", report_progress_fn)?;

        // set up sys.settrace for automatic cancellation checking
        // this checks cancellation at each line of Python code
        let cancelled_for_trace = ctx.cancelled.clone();
        let check_cancellation_fn = PyCFunction::new_closure(
            py,
            Some(c"_flow_check_cancellation"),
            None,
            move |_args: &Bound<'_, pyo3::types::PyTuple>,
                  _kwargs: Option<&Bound<'_, PyDict>>|
                  -> PyResult<()> {
                if cancelled_for_trace.load(Ordering::Relaxed) {
                    tracing::info!("[python] script cancellation triggered via trace callback");
                    return Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                        "script execution cancelled",
                    ));
                }
                Ok(())
            },
        )?;
        globals.set_item("_flow_check_cancellation", check_cancellation_fn)?;

        // define trace function in Python to correctly return itself
        py.run(
            c"
import sys
def _flow_trace(frame, event, arg):
    _flow_check_cancellation()
    return _flow_trace
sys.settrace(_flow_trace)
",
            Some(&globals),
            Some(&globals),
        )?;

        // execute the script in the module namespace
        py.run(
            &std::ffi::CString::new(script).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid script: {}", e))
            })?,
            Some(&globals),
            Some(&globals),
        )?;

        Ok(globals)
    }

    fn json_to_pyobject<'py>(py: Python<'py>, value: &JsonValue) -> PyResult<Bound<'py, PyAny>> {
        // use pythonize for simpler JSON to Python conversion logic (manual here)
        match value {
            JsonValue::Null => Ok(py.None().into_bound(py)),
            JsonValue::Bool(b) => Ok((*b).into_pyobject(py)?.to_owned().into_any()),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(i.into_pyobject(py)?.to_owned().into_any())
                } else if let Some(f) = n.as_f64() {
                    Ok(f.into_pyobject(py)?.to_owned().into_any())
                } else {
                    Ok(py.None().into_bound(py))
                }
            }
            JsonValue::String(s) => Ok(s.as_str().into_pyobject(py)?.to_owned().into_any()),
            JsonValue::Array(arr) => {
                let list = PyList::empty(py);
                for item in arr {
                    list.append(Self::json_to_pyobject(py, item)?)?;
                }
                Ok(list.into_any())
            }
            JsonValue::Object(obj) => {
                let dict = PyDict::new(py);
                for (k, v) in obj {
                    dict.set_item(k, Self::json_to_pyobject(py, v)?)?;
                }
                Ok(dict.into_any())
            }
        }
    }

    fn pyobject_to_json(obj: &Bound<'_, PyAny>) -> PyResult<JsonValue> {
        if obj.is_none() {
            Ok(JsonValue::Null)
        } else if let Ok(b) = obj.extract::<bool>() {
            Ok(JsonValue::Bool(b))
        } else if let Ok(i) = obj.extract::<i64>() {
            Ok(JsonValue::Number(i.into()))
        } else if let Ok(f) = obj.extract::<f64>() {
            Ok(serde_json::Number::from_f64(f)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null))
        } else if let Ok(s) = obj.extract::<String>() {
            Ok(JsonValue::String(s))
        } else if let Ok(list) = obj.downcast::<PyList>() {
            let arr: PyResult<Vec<JsonValue>> = list
                .iter()
                .map(|item| Self::pyobject_to_json(&item))
                .collect();
            Ok(JsonValue::Array(arr?))
        } else if let Ok(dict) = obj.downcast::<PyDict>() {
            let mut map = serde_json::Map::new();
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                map.insert(key, Self::pyobject_to_json(&v)?);
            }
            Ok(JsonValue::Object(map))
        } else {
            // try to convert to string as fallback
            Ok(JsonValue::String(obj.str()?.to_string()))
        }
    }
}

impl ScriptEngine for PythonEngine {
    fn language(&self) -> &str {
        "python"
    }

    fn parse_spec(&self, script: &str, filename: &str) -> Result<ScriptSpec> {
        Python::with_gil(|py| {
            let module = PyModule::new(py, "user_script")?;
            let globals = Self::setup_and_run_script(py, script, &module)
                .map_err(|e| anyhow!("failed to execute Python script {}: {}", filename, e))?;

            // get the spec function and call it
            let spec_fn = globals
                .get_item("spec")
                .map_err(|e| anyhow!("failed to get spec function: {}", e))?
                .ok_or_else(|| anyhow!("missing spec() function in {}", filename))?;

            let spec_result = spec_fn
                .call0()
                .map_err(|e| anyhow!("failed to call spec() in {}: {}", filename, e))?;

            // convert to JSON and use common parser
            let json_val = Self::pyobject_to_json(&spec_result)
                .map_err(|e| anyhow!("failed to convert spec result to JSON: {}", e))?;

            parse_spec_from_json(json_val)
        })
    }

    fn execute(
        &self,
        script: &str,
        inputs: HashMap<String, JsonValue>,
        ctx: Arc<ScriptContext>,
    ) -> Result<HashMap<String, JsonValue>> {
        // check if already cancelled before starting
        if ctx.is_cancelled() {
            info!("[python] script execution skipped - already cancelled");
            return Err(anyhow!("script execution cancelled"));
        }

        debug!("[python] starting script execution");
        Python::with_gil(|py| {
            let module = PyModule::new(py, "user_script")?;
            let globals =
                Self::setup_and_run_script_with_context(py, script, &module, ctx.clone())?;

            let execute_fn = globals
                .get_item("execute")?
                .ok_or_else(|| anyhow!("missing execute() function"))?;

            let inputs_dict = PyDict::new(py);
            for (k, v) in inputs {
                inputs_dict.set_item(k, Self::json_to_pyobject(py, &v)?)?;
            }

            let result_obj = execute_fn.call1((inputs_dict,));

            // clear the trace function regardless of success/failure
            debug!("[python] cleaning up trace function");
            match py.run(
                c"import sys; sys.settrace(None)",
                Some(&globals),
                Some(&globals),
            ) {
                Ok(_) => debug!("[python] trace function cleared"),
                Err(e) => tracing::error!("[python] failed to clear trace function: {}", e),
            }

            // explicitly clear globals to break potential cycles
            globals.clear();

            let result = result_obj.map_err(|e| {
                // check if this was a cancellation (KeyboardInterrupt)
                if e.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py) {
                    info!("[python] script execution cancelled");
                    anyhow!("script execution cancelled")
                } else {
                    anyhow!("failed to call execute(): {}", e)
                }
            })?;

            let json_val = Self::pyobject_to_json(&result)
                .map_err(|e| anyhow!("failed to convert execute result to JSON: {}", e))?;

            let result_map = json_val
                .as_object()
                .ok_or_else(|| anyhow!("script execution must return an object"))?;

            Ok(result_map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        })
    }

    fn get_options(
        &self,
        script: &str,
        input_name: &str,
        inputs: HashMap<String, JsonValue>,
    ) -> Result<Vec<SelectOption>> {
        Python::with_gil(|py| {
            let module = PyModule::new(py, "user_script")?;
            let globals = Self::setup_and_run_script(py, script, &module)?;

            let get_options_fn = match globals.get_item("get_options")? {
                Some(f) => f,
                None => return Ok(vec![]),
            };

            let inputs_dict = PyDict::new(py);
            for (k, v) in inputs {
                inputs_dict.set_item(k, Self::json_to_pyobject(py, &v)?)?;
            }

            let result = get_options_fn.call1((input_name, inputs_dict))?;

            let json_val = Self::pyobject_to_json(&result)
                .map_err(|e| anyhow!("failed to convert get_options result to JSON: {}", e))?;

            let arr = json_val
                .as_array()
                .ok_or_else(|| anyhow!("get_options must return an array"))?;

            parse_select_options_list(arr)
        })
    }
}
