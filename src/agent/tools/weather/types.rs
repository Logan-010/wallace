use jiff::{ToSpan, civil::Date};
use serde::Deserialize;
use std::fmt;

pub const WEATHER_URL: &str = "https://api.open-meteo.com/v1/forecast";
pub const LOCATION_URL: &str = "https://ipwho.is/";

#[derive(Debug)]
pub struct WeatherError(pub(super) String);

impl fmt::Display for WeatherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WeatherError {}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Current,
    Daily,
    Hourly,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherArgs {
    #[serde(default)]
    pub view: View,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub days: Option<u8>,
    pub day_offset: Option<u8>,
    #[serde(default)]
    pub units: Units,
}

impl WeatherArgs {
    pub(super) fn validate(&self) -> Result<(), WeatherError> {
        if self.start_date.is_some() && self.day_offset.is_some() {
            return Err(WeatherError(
                "Use start_date or day_offset, not both.".into(),
            ));
        }
        if self.end_date.is_some() && self.days.is_some() {
            return Err(WeatherError("Use end_date or days, not both.".into()));
        }
        if self.days.is_some_and(|days| !(1..=16).contains(&days)) {
            return Err(WeatherError("days must be between 1 and 16.".into()));
        }
        if self.day_offset.is_some_and(|offset| offset > 15) {
            return Err(WeatherError("day_offset must be between 0 and 15.".into()));
        }
        if self.view == View::Current
            && (self.start_date.is_some()
                || self.end_date.is_some()
                || self.days.is_some()
                || self.day_offset.is_some())
        {
            return Err(WeatherError(
                "Current weather does not accept a date range; use daily or hourly.".into(),
            ));
        }
        for date in [&self.start_date, &self.end_date].into_iter().flatten() {
            parse_date(date)?;
        }
        Ok(())
    }

    pub(super) fn range(&self, today: Date) -> Result<(Date, Date), WeatherError> {
        self.validate()?;
        let start = match &self.start_date {
            Some(date) => parse_date(date)?,
            None => today
                .checked_add(i64::from(self.day_offset.unwrap_or(0)).days())
                .map_err(|e| WeatherError(e.to_string()))?,
        };
        let end = match &self.end_date {
            Some(date) => parse_date(date)?,
            None => {
                let default_days = if self.view == View::Daily
                    && self.start_date.is_none()
                    && self.day_offset.is_none()
                {
                    7
                } else {
                    1
                };
                start
                    .checked_add(i64::from(self.days.unwrap_or(default_days) - 1).days())
                    .map_err(|e| WeatherError(e.to_string()))?
            }
        };
        let last = today
            .checked_add(15.days())
            .map_err(|e| WeatherError(e.to_string()))?;
        if start < today || end < start || end > last {
            return Err(WeatherError(format!(
                "Forecast dates must be ordered and within {today} through {last} (16 days including today)."
            )));
        }
        Ok((start, end))
    }
}

fn parse_date(value: &str) -> Result<Date, WeatherError> {
    let date = value
        .parse::<Date>()
        .map_err(|_| WeatherError("Dates must be valid YYYY-MM-DD dates.".into()))?;
    if date.to_string() != value {
        return Err(WeatherError("Dates must use YYYY-MM-DD format.".into()));
    }
    Ok(date)
}

pub(super) fn weather_description(code: u64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Depositing rime fog",
        51 => "Light drizzle",
        53 => "Moderate drizzle",
        55 => "Dense drizzle",
        56 => "Light freezing drizzle",
        57 => "Dense freezing drizzle",
        61 => "Slight rain",
        63 => "Moderate rain",
        65 => "Heavy rain",
        66 => "Light freezing rain",
        67 => "Heavy freezing rain",
        71 => "Slight snowfall",
        73 => "Moderate snowfall",
        75 => "Heavy snowfall",
        77 => "Snow grains",
        80 => "Slight rain showers",
        81 => "Moderate rain showers",
        82 => "Violent rain showers",
        85 => "Slight snow showers",
        86 => "Heavy snow showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm with slight hail",
        97 => "Heavy thunderstorm",
        99 => "Thunderstorm with heavy hail",
        _ => "Unknown weather condition",
    }
}
