use jiff::ZonedDateTime;
use rig::tool::{Tool, ToolContext};
use std::convert::Infallible;

pub struct DateTimeTool;

impl Tool for DateTimeTool {
    const NAME: &'static str = "datetime";
    type Args = ();
    type Output = String;
    type Error = Infallible;

    fn description(&self) -> String {
        "Get the current date and time".to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(ZonedDateTime::now().to_string())
    }
}
