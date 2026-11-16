use super::tool::WeatherTool;
use super::types::{
    LOCATION_URL, Units, View, WEATHER_URL, WeatherArgs, WeatherError, weather_description,
};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{Value, json};

const CURRENT: &str = "temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,rain,showers,snowfall,weather_code,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m";
const DAILY: &str = "weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,apparent_temperature_min,sunrise,sunset,daylight_duration,sunshine_duration,uv_index_max,precipitation_sum,rain_sum,showers_sum,snowfall_sum,precipitation_hours,precipitation_probability_max,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant";
const HOURLY: &str = "temperature_2m,relative_humidity_2m,dew_point_2m,apparent_temperature,precipitation_probability,precipitation,rain,showers,snowfall,snow_depth,weather_code,pressure_msl,cloud_cover,visibility,wind_speed_10m,wind_direction_10m,wind_gusts_10m,uv_index,is_day";

impl WeatherTool {
    async fn get_json(&self, url: &str, query: &[(String, String)]) -> Result<Value, WeatherError> {
        let response = self
            .client
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(|e| WeatherError(format!("Weather service request failed: {e}")))?;
        let status = response.status();
        let data: Value = response.json().await.map_err(|e| {
            WeatherError(format!(
                "Weather service returned invalid JSON ({status}): {e}"
            ))
        })?;
        if !status.is_success() || data["error"] == true || data["success"] == false {
            let reason = data["reason"]
                .as_str()
                .or_else(|| data["message"].as_str())
                .unwrap_or("Service unavailable");
            return Err(WeatherError(format!(
                "Weather service error ({status}): {reason}"
            )));
        }
        Ok(data)
    }

    pub(super) async fn fetch(&self, args: WeatherArgs) -> Result<Value, WeatherError> {
        self.fetch_with_urls(args, LOCATION_URL, WEATHER_URL).await
    }

    pub(super) async fn fetch_with_urls(
        &self,
        args: WeatherArgs,
        location_url: &str,
        weather_url: &str,
    ) -> Result<Value, WeatherError> {
        args.validate()?;
        let location = self.get_json(location_url, &[]).await.map_err(|e| {
            WeatherError(format!("Unable to determine location automatically: {e}"))
        })?;
        let location: Location = serde_json::from_value(location).map_err(|e| {
            WeatherError(format!(
                "Location service returned incomplete location data: {e}"
            ))
        })?;
        location.validate()?;
        let today = Timestamp::now()
            .in_tz(&location.timezone.id)
            .map_err(|e| WeatherError(format!("Invalid location time zone: {e}")))?
            .date();
        let (start, end) = args.range(today)?;
        let mut query = vec![
            ("latitude".into(), location.latitude.to_string()),
            ("longitude".into(), location.longitude.to_string()),
            ("timezone".into(), location.timezone.id.clone()),
            ("start_date".into(), start.to_string()),
            ("end_date".into(), end.to_string()),
            ("daily".into(), DAILY.into()),
        ];
        match args.units {
            Units::Metric => query.extend([
                ("temperature_unit".into(), "celsius".into()),
                ("wind_speed_unit".into(), "kmh".into()),
                ("precipitation_unit".into(), "mm".into()),
            ]),
            Units::Imperial => query.extend([
                ("temperature_unit".into(), "fahrenheit".into()),
                ("wind_speed_unit".into(), "mph".into()),
                ("precipitation_unit".into(), "inch".into()),
            ]),
        }
        match args.view {
            View::Current => query.push(("current".into(), CURRENT.into())),
            View::Hourly => query.push(("hourly".into(), HOURLY.into())),
            View::Daily => {}
        }
        let mut forecast = self.get_json(weather_url, &query).await?;
        validate_forecast(&forecast, args.view)?;
        add_descriptions(&mut forecast);
        Ok(json!({
            "source": "Open-Meteo", "source_url": "https://open-meteo.com/",
            "location": {
                "city": location.city, "region": location.region, "country": location.country,
                "latitude": location.latitude, "longitude": location.longitude,
                "method": "public_ip", "approximate": true,
            },
            "start_date": start.to_string(), "end_date": end.to_string(),
            "notes": "Location is approximate and may reflect a VPN or proxy. Current conditions are model estimates, not a station observation. Hourly precipitation covers the preceding hour. Use returned units; null means unavailable, not zero. Local timestamps use the returned fixed UTC offset, including across DST changes. This forecast is not an official severe-weather alert service.",
            "forecast": forecast,
        }))
    }
}

#[derive(Deserialize)]
struct Location {
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    city: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    country: String,
    timezone: LocationTimezone,
}

#[derive(Deserialize)]
struct LocationTimezone {
    id: String,
}

impl Location {
    fn validate(&self) -> Result<(), WeatherError> {
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
        {
            return Err(WeatherError(
                "Location service returned invalid coordinates.".into(),
            ));
        }
        Ok(())
    }
}

fn validate_forecast(data: &Value, view: View) -> Result<(), WeatherError> {
    let mut sections = vec!["daily"];
    if view == View::Hourly {
        sections.push("hourly");
    }
    for section in sections {
        let times = data[section]["time"]
            .as_array()
            .filter(|times| !times.is_empty())
            .ok_or_else(|| {
                WeatherError(format!("Weather service returned no {section} forecast."))
            })?;
        let object = data[section]
            .as_object()
            .expect("time array implies object");
        if object.values().any(|values| {
            values
                .as_array()
                .is_none_or(|values| values.len() != times.len())
        }) {
            return Err(WeatherError(format!(
                "Weather service returned inconsistent {section} arrays."
            )));
        }
    }
    if view == View::Current && !data["current"]["time"].is_string() {
        return Err(WeatherError(
            "Weather service returned no current conditions.".into(),
        ));
    }
    Ok(())
}

fn add_descriptions(data: &mut Value) {
    if let Some(current) = data["current"].as_object_mut() {
        current.insert(
            "weather_description".into(),
            current
                .get("weather_code")
                .and_then(Value::as_u64)
                .map(|code| json!(weather_description(code)))
                .unwrap_or(Value::Null),
        );
    }
    for section in ["daily", "hourly"] {
        if let Some(table) = data[section].as_object_mut()
            && let Some(codes) = table.get("weather_code").and_then(Value::as_array)
        {
            let descriptions: Vec<Value> = codes
                .iter()
                .map(|code| {
                    code.as_u64()
                        .map(|code| json!(weather_description(code)))
                        .unwrap_or(Value::Null)
                })
                .collect();
            table.insert("weather_description".into(), json!(descriptions));
        }
    }
}
