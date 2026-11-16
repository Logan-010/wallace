use super::types::{WeatherArgs, WeatherError};
use rig::tool::{Tool, ToolContext};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone)]
pub struct WeatherTool {
    pub(super) client: reqwest::Client,
}

impl WeatherTool {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("wallace-weather/0.1")
                .build()?,
        })
    }
}

impl Tool for WeatherTool {
    const NAME: &'static str = "weather";
    type Args = WeatherArgs;
    type Output = Value;
    type Error = WeatherError;

    fn description(&self) -> String {
        "Get Open-Meteo weather for the automatically detected public-IP location. Current (default) includes today's forecast; daily gives the next 7 days by default; hourly gives a whole day's hourly forecast. Use day_offset=1 for tomorrow, start_date for a specific local date, days for the next N days, or start_date/end_date for an inclusive range. Up to 16 days including today; no historical weather. Resolve weekdays and calendar weeks using the location's local date, not UTC. Includes feels-like temperature, rain/snow probabilities and amounts, wind/gusts, humidity, UV, visibility, sunrise/sunset and WMO descriptions where available. Not official weather alerts. Never invent missing values.".into()
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object", "additionalProperties": false, "required": [],
            "properties": {
                "view": {"type": "string", "enum": ["current", "daily", "hourly"], "description": "Default current; use daily for today/tomorrow, a date, several days or a week; hourly for time-of-day questions."},
                "start_date": {"type": "string", "format": "date", "description": "First local date YYYY-MM-DD; alone returns one day. Cannot combine with day_offset."},
                "end_date": {"type": "string", "format": "date", "description": "Last local date, inclusive. Cannot combine with days."},
                "days": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Number of days starting today or at the requested start; 7 for a rolling week."},
                "day_offset": {"type": "integer", "minimum": 0, "maximum": 15, "description": "Days from today in detected location: 0 today, 1 tomorrow. Alone returns one day."},
                "units": {"type": "string", "enum": ["metric", "imperial"], "description": "Default metric (C, km/h, mm); imperial uses F, mph, inches. Read units for snow and visibility from response."}
            }
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        self.fetch(args).await
    }
}
