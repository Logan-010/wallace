use jiff::ZonedDateTime;
use rig::tool::{Tool, ToolContext};
use serde::Deserialize;
use std::convert::Infallible;

pub struct DateTimeTool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DateTimeArgs {}

impl Tool for DateTimeTool {
    const NAME: &'static str = "datetime";
    type Args = DateTimeArgs;
    type Output = String;
    type Error = Infallible;

    fn description(&self) -> String {
        "Get the current date and time in the system's local time zone, including the UTC offset."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
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
