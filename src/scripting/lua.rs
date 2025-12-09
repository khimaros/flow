use super::*;

pub struct LuaEngine;

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaEngine {
    pub fn new() -> Self {
        Self
    }
}

impl ScriptEngine for LuaEngine {
    fn language(&self) -> &str {
        "lua"
    }

    fn parse_spec(&self, _script: &str, _filename: &str) -> Result<ScriptSpec> {
        Err(anyhow::anyhow!("lua support not yet implemented"))
    }

    fn execute(
        &self,
        _script: &str,
        _inputs: HashMap<String, JsonValue>,
        _ctx: Arc<ScriptContext>,
    ) -> Result<HashMap<String, JsonValue>> {
        Err(anyhow::anyhow!("lua support not yet implemented"))
    }

    fn get_options(
        &self,
        _script: &str,
        _input_name: &str,
        _inputs: HashMap<String, JsonValue>,
    ) -> Result<Vec<SelectOption>> {
        Ok(vec![])
    }
}
