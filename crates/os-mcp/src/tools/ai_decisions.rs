//! STUB(M4-C02): cost-class wrapper for run_decisions; no network or consent mutation.
use crate::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};
pub struct AiTool;
impl Tool for AiTool {
    fn name(&self) -> &'static str {
        "run_decisions"
    }
    fn class(&self) -> ToolClass {
        ToolClass::Cost
    }
    fn description(&self) -> &'static str {
        "AI task (available with M4-C02). Requires existing data consent and separate cost confirmation."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"crawl_id":{"type":"string"},"confirm":{"type":"boolean"}},"required":["crawl_id"],"additionalProperties":false})
    }
    fn call(&self, _: &ToolContext, _: Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::InvalidArguments(
            "STUB(M4-C02): AI task execution is not available".into(),
        ))
    }
}
