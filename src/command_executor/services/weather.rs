use std::time::Duration;

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

use url::Url;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(rename = "generationtime_ms")]
    pub generationtime_ms: f64,
    #[serde(rename = "utc_offset_seconds")]
    pub utc_offset_seconds: i64,
    pub timezone: String,
    #[serde(rename = "timezone_abbreviation")]
    pub timezone_abbreviation: String,
    pub elevation: f64,
    #[serde(rename = "current_units")]
    pub current_units: CurrentUnits,
    pub current: Current,
    #[serde(rename = "hourly_units")]
    pub hourly_units: HourlyUnits,
    pub hourly: Hourly,
    #[serde(rename = "daily_units")]
    pub daily_units: DailyUnits,
    pub daily: Daily,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUnits {
    pub time: String,
    pub interval: String,
    #[serde(rename = "temperature_2m")]
    pub temperature_2m: String,
    pub snowfall: String,
    pub showers: String,
    pub rain: String,
    pub precipitation: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub time: String,
    pub interval: i64,
    #[serde(rename = "temperature_2m")]
    pub temperature_2m: f64,
    pub snowfall: f64,
    pub showers: f64,
    pub rain: f64,
    pub precipitation: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourlyUnits {
    pub time: String,
    #[serde(rename = "temperature_2m")]
    pub temperature_2m: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hourly {
    pub time: Vec<String>,
    #[serde(rename = "temperature_2m")]
    pub temperature_2m: Vec<f64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUnits {
    pub time: String,
    #[serde(rename = "temperature_2m_min")]
    pub temperature_2m_min: String,
    #[serde(rename = "temperature_2m_max")]
    pub temperature_2m_max: String,
    #[serde(rename = "snowfall_sum")]
    pub snowfall_sum: String,
    #[serde(rename = "showers_sum")]
    pub showers_sum: String,
    #[serde(rename = "precipitation_sum")]
    pub precipitation_sum: String,
    #[serde(rename = "precipitation_hours")]
    pub precipitation_hours: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily {
    pub time: Vec<String>,
    #[serde(rename = "temperature_2m_min")]
    pub temperature_2m_min: Vec<f64>,
    #[serde(rename = "temperature_2m_max")]
    pub temperature_2m_max: Vec<f64>,
    #[serde(rename = "snowfall_sum")]
    pub snowfall_sum: Vec<f64>,
    #[serde(rename = "showers_sum")]
    pub showers_sum: Vec<f64>,
    #[serde(rename = "precipitation_sum")]
    pub precipitation_sum: Vec<f64>,
    #[serde(rename = "precipitation_hours")]
    pub precipitation_hours: Vec<f64>,
    #[serde(rename = "weather_code")]
    pub weather_code: Vec<i32>,
}

fn weather_code_to_string(weather_code: i32) -> String {
    // https://open-meteo.com/en/docs
    // https://www.nodc.noaa.gov/archive/arc0021/0002199/1.1/data/0-data/HTML/WMO-CODE/WMO4677.HTM
    match weather_code {
        0 => "Clear sky".to_string(),
        1 => "Mainly clear".to_string(),
        2 => "Partly cloudy".to_string(),
        3 => "Overcast".to_string(),
        45 | 48 => "Foggy".to_string(),
        51 => "Drizzle, Light intensity".to_string(),
        53 => "Drizzle, Moderate intensity".to_string(),
        55 => "Drizzle, Dense intensity".to_string(),
        56 => "Freezing Drizzle, Light intensity".to_string(),
        57 => "Freezing Drizzle, Dense intensity".to_string(),
        61 => "Rain, Light intensity".to_string(),
        63 => "Rain, Moderate intensity".to_string(),
        65 => "Rain, Heavy intensity".to_string(),
        66 => "Freezing Rain, Light intensity".to_string(),
        67 => "Freezing Rain, heavy intensity".to_string(),
        71 => "Snow fall, Light intensity".to_string(),
        73 => "Snow fall, Moderate intensity".to_string(),
        75 => "Snow fall, Heavy intensity".to_string(),
        77 => "Snow grains".to_string(),
        80 => "Rain showers, Light intensity".to_string(),
        81 => "Rain showers, Moderate intensity".to_string(),
        82 => "Rain showers, Heavy intensity".to_string(),
        85 => "Snow showers, Light intensity".to_string(),
        86 => "Snow showers, Heavy intensity".to_string(),
        95 => "Thunderstorm, Slight or moderate".to_string(),
        96 => "Thunderstorm with slight hail".to_string(),
        99 => "Thunderstorm with heavy hail".to_string(),
        _ => "Unknown weather condition".to_string(),
    }
}

pub fn get_weather(latitude: f64, longitude: f64) -> Result<String> {
    let url_params = [
        ("latitude", latitude.to_string()),
        ("longitude", longitude.to_string()),
        ("current", "temperature_2m,snowfall,showers,rain,precipitation".to_string()),
        ("hourly", "temperature_2m".to_string()),
        ("daily", "temperature_2m_min,temperature_2m_max,snowfall_sum,showers_sum,precipitation_sum,precipitation_hours,weather_code".to_string()),
        ("timezone", "America/Denver".to_string()),
        ("wind_speed_unit", "mph".to_string()),
        ("temperature_unit", "fahrenheit".to_string()),
        ("precipitation_unit", "inch".to_string()),
    ];

    let url = Url::parse_with_params("https://api.open-meteo.com/v1/forecast", &url_params)?;
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()?
        .error_for_status()?;
    let body = response.text()?;
    let weather: Root = serde_json::from_str(&body)?;

    let current = &weather.current;
    let daily = &weather.daily;

    // We'll use today's information (index 0) from the daily values
    let today_min = daily.temperature_2m_min.get(0).copied().unwrap_or(f64::NAN);
    let today_max = daily.temperature_2m_max.get(0).copied().unwrap_or(f64::NAN);
    let today_snowfall = daily.snowfall_sum.get(0).copied().unwrap_or(0.0);
    let today_precip = daily.precipitation_sum.get(0).copied().unwrap_or(0.0);

    let mut summary = format!(
        "The current temperature is {:.0} degrees. Today's high will be {:.0} degrees and the low will be {:.0} degrees. The weather is {}.",
        current.temperature_2m,
        today_max,
        today_min,
        weather_code_to_string(weather.daily.weather_code.get(0).copied().unwrap_or(0))
    );

    if today_precip > 0.0 {
        summary.push_str(&format!(
            " Expected total precipitation is {:.2} inches.",
            today_precip
        ));
    }
    if today_snowfall > 0.0 {
        summary.push_str(&format!(
            " Snowfall is expected to total {:.2} inches.",
            today_snowfall
        ));
    }

    Ok(summary)
}
