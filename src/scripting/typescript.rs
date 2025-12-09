use super::*;
use boa_engine::{
    context::ContextBuilder, js_string, object::ObjectInitializer, property::Attribute, Context,
    JsResult, JsValue, NativeFunction, Source,
};
use serde_json::Value as JsonValue;

pub struct TypeScriptEngine;

impl Default for TypeScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptEngine {
    pub fn new() -> Self {
        Self
    }

    fn create_context() -> Result<Context> {
        let mut context = ContextBuilder::default()
            .build()
            .map_err(|e| anyhow!("failed to create JS context: {}", e))?;

        // register console.log
        let console = ObjectInitializer::new(&mut context)
            .function(
                NativeFunction::from_fn_ptr(|_this, args, context| {
                    let msg = args
                        .first()
                        .map(|v| {
                            v.to_string(context)
                                .unwrap_or_default()
                                .to_std_string_escaped()
                        })
                        .unwrap_or_default();
                    tracing::info!("[typescript] {}", msg);
                    Ok(JsValue::undefined())
                }),
                js_string!("log"),
                0,
            )
            .build();

        context
            .register_global_property(js_string!("console"), console, Attribute::all())
            .map_err(|e| anyhow!("failed to register console: {}", e))?;

        // register crypto.randomUUID
        let crypto = ObjectInitializer::new(&mut context)
            .function(
                NativeFunction::from_fn_ptr(|_this, _args, _context| {
                    let uuid = uuid::Uuid::new_v4().to_string();
                    Ok(JsValue::from(js_string!(uuid)))
                }),
                js_string!("randomUUID"),
                0,
            )
            .build();

        context
            .register_global_property(js_string!("crypto"), crypto, Attribute::all())
            .map_err(|e| anyhow!("failed to register crypto: {}", e))?;

        Ok(context)
    }

    fn json_to_js(context: &mut Context, json: &JsonValue) -> JsResult<JsValue> {
        JsValue::from_json(json, context)
    }

    fn js_to_json(context: &mut Context, js: &JsValue) -> JsResult<JsonValue> {
        js.to_json(context)
    }
}

impl ScriptEngine for TypeScriptEngine {
    fn language(&self) -> &str {
        "typescript"
    }

    fn parse_spec(&self, script: &str, filename: &str) -> Result<ScriptSpec> {
        let mut context = Self::create_context()?;

        // execute script to load functions
        context
            .eval(Source::from_bytes(script))
            .map_err(|e| anyhow!("failed to eval script {}: {}", filename, e))?;

        let global = context.global_object();
        let spec_fn = global
            .get(js_string!("spec"), &mut context)
            .map_err(|e| anyhow!("failed to get spec function: {}", e))?;

        if !spec_fn.is_callable() {
            return Err(anyhow!("'spec' is not a function in {}", filename));
        }

        let result = spec_fn
            .as_callable()
            .unwrap()
            .call(&JsValue::undefined(), &[], &mut context)
            .map_err(|e| anyhow!("failed to call spec(): {}", e))?;

        let json_val = Self::js_to_json(&mut context, &result)
            .map_err(|e| anyhow!("failed to convert spec result to JSON: {}", e))?;

        parse_spec_from_json(json_val)
    }

    fn execute(
        &self,
        script: &str,
        inputs: HashMap<String, JsonValue>,
        _ctx: Arc<ScriptContext>,
    ) -> Result<HashMap<String, JsonValue>> {
        let mut context = Self::create_context()?;

        context
            .eval(Source::from_bytes(script))
            .map_err(|e| anyhow!("failed to eval script: {}", e))?;

        let global = context.global_object();
        let execute_fn = global
            .get(js_string!("execute"), &mut context)
            .map_err(|e| anyhow!("failed to get execute function: {}", e))?;

        if !execute_fn.is_callable() {
            return Err(anyhow!("'execute' is not a function"));
        }

        // convert inputs
        let inputs_json = serde_json::Value::Object(inputs.into_iter().collect());
        let inputs_js = Self::json_to_js(&mut context, &inputs_json)
            .map_err(|e| anyhow!("failed to convert inputs to JS: {}", e))?;

        let result = execute_fn
            .as_callable()
            .unwrap()
            .call(&JsValue::undefined(), &[inputs_js], &mut context)
            .map_err(|e| anyhow!("failed to call execute(): {}", e))?;

        let result_json = Self::js_to_json(&mut context, &result)
            .map_err(|e| anyhow!("failed to convert execute result to JSON: {}", e))?;

        let result_map = result_json
            .as_object()
            .ok_or_else(|| anyhow!("script execution must return an object"))?;

        Ok(result_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn get_options(
        &self,
        script: &str,
        input_name: &str,
        inputs: HashMap<String, JsonValue>,
    ) -> Result<Vec<SelectOption>> {
        let mut context = Self::create_context()?;

        context
            .eval(Source::from_bytes(script))
            .map_err(|e| anyhow!("failed to eval script: {}", e))?;

        let global = context.global_object();
        let func = global.get(js_string!("get_options"), &mut context);

        // check if function exists
        match func {
            Ok(f) if f.is_callable() => {
                let inputs_json = serde_json::Value::Object(inputs.into_iter().collect());
                let inputs_js = Self::json_to_js(&mut context, &inputs_json)
                    .map_err(|e| anyhow!("failed to convert inputs: {}", e))?;
                let name_js = JsValue::from(js_string!(input_name));

                let result = f
                    .as_callable()
                    .unwrap()
                    .call(&JsValue::undefined(), &[name_js, inputs_js], &mut context)
                    .map_err(|e| anyhow!("failed to call get_options(): {}", e))?;

                let result_json = Self::js_to_json(&mut context, &result)
                    .map_err(|e| anyhow!("failed to convert result: {}", e))?;
                let arr = result_json
                    .as_array()
                    .ok_or_else(|| anyhow!("get_options must return an array"))?;

                parse_select_options_list(arr)
            }
            _ => Ok(vec![]),
        }
    }
}
