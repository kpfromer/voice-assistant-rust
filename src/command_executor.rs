use std::time::Duration;

use chrono::Datelike;
use chrono::Local;
use chrono::Timelike;
use color_eyre::eyre::{Context, Result};
use pest::Parser;
use pest_derive::Parser;
use serde_json::json;
use url::Url;

use crate::human_format::int_to_words;

#[derive(Parser)]
#[grammar = "command_grammar.pest"]
struct CommandParser;

#[derive(Debug)]
enum Intent {
    TurnOnLight { area: Option<String> },
    TurnOffLight { area: Option<String> },
    GetCurrentTime,
    Unknown,
}

fn parse_intent(command: &str) -> Intent {
    let command_lower = command.to_lowercase();
    let pairs = match CommandParser::parse(Rule::command, &command_lower) {
        Ok(mut pairs) => pairs.next().unwrap(),
        Err(e) => {
            eprintln!("Parse error: {:?}", e);
            return Intent::Unknown;
        }
    };

    match pairs.as_rule() {
        Rule::turn_on_lights_command => {
            let mut inner = pairs.into_inner();
            inner.next(); // Skip turn_on
            let area = extract_area(&mut inner);
            Intent::TurnOnLight { area }
        }
        Rule::turn_off_lights_command => {
            let mut inner = pairs.into_inner();
            inner.next(); // Skip turn_off
            let area = extract_area(&mut inner);
            Intent::TurnOffLight { area }
        }
        Rule::current_time_command => Intent::GetCurrentTime,
        _ => return Intent::Unknown,
    }
}

fn extract_area(pairs: &mut pest::iterators::Pairs<'_, Rule>) -> Option<String> {
    let pair = pairs.next()?;

    match pair.as_rule() {
        Rule::all_lights | Rule::lights_only => None,
        Rule::lights_with_area => {
            // Structure: light_word ~ whitespace ~ area_prefix ~ area_name
            // Skip light_word and area_prefix to get to area_name
            let mut inner = pair.into_inner();
            inner.next(); // Skip light_word
            inner.next(); // Skip area_prefix
            let area_pair = inner.next()?;

            // Extract the area name from the matched text
            let area_text = area_pair.as_str();
            Some(normalize_area_name(area_text))
        }
        Rule::lights_with_area_before => {
            // Structure: area_name ~ whitespace ~ light_word
            // area_name is the first element
            let mut inner = pair.into_inner();
            let area_pair = inner.next()?;

            // Extract the area name from the matched text
            let area_text = area_pair.as_str();
            Some(normalize_area_name(area_text))
        }
        _ => None,
    }
}

fn normalize_area_name(area: &str) -> String {
    // Convert area names from grammar to Home Assistant area IDs
    match area.trim() {
        "living room" => "living_room".to_string(),
        "bedroom" => "bedroom".to_string(),
        "hallway" => "hallway".to_string(),
        "kitchen" => "kitchen".to_string(),
        other => other.replace(' ', "_"),
    }
}

fn turn_on_light(config: &CommandExecutorConfig, area: Option<String>) -> Result<()> {
    let client = reqwest::blocking::Client::new();
    let url = config
        .home_assistant_base_url
        .join("/api/services/light/turn_on")?;

    let body = if let Some(area_id) = area {
        json!({
            "area_id": area_id
        })
    } else {
        json!({
            "entity_id": "all"
        })
    };

    // TODO: use async version
    let _ = client
        .post(url)
        .header(
            "Authorization",
            format!("Bearer {}", &config.home_assistant_token),
        )
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).wrap_err("failed to serialize request body")?)
        .timeout(Duration::from_secs(5))
        .send()?
        .error_for_status();

    Ok(())
}

fn turn_off_light(config: &CommandExecutorConfig, area: Option<String>) -> Result<()> {
    let client = reqwest::blocking::Client::new();
    let url = config
        .home_assistant_base_url
        .join("/api/services/light/turn_off")?;

    let body = if let Some(area_id) = area {
        json!({
            "area_id": area_id
        })
    } else {
        json!({
            "entity_id": "all"
        })
    };

    // TODO: use async version
    let _ = client
        .post(url)
        .header(
            "Authorization",
            format!("Bearer {}", &config.home_assistant_token),
        )
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).wrap_err("failed to serialize request body")?)
        .timeout(Duration::from_secs(5))
        .send()?
        .error_for_status();

    Ok(())
}

fn get_current_time() -> Result<String> {
    let now = Local::now();
    let time_of_day = now.time();

    let hour = time_of_day.hour();
    let rounded_hour = hour % 12;
    let hour_str = if rounded_hour == 0 {
        int_to_words(12)
    } else {
        int_to_words(rounded_hour as i32)
    };
    let am_pm_str = if hour < 12 { "AM" } else { "PM" };
    let minute_str = int_to_words(time_of_day.minute() as i32);

    let month_str = match now.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => unreachable!(),
    };
    let day_str = int_to_words(now.day() as i32);

    Ok(format!(
        "It is {hour_str} {minute_str} {am_pm_str}. Date is {month_str} {day_str}.",
    ))
}
pub struct CommandExecutorConfig {
    // TODO: use Url
    home_assistant_base_url: Url,
    // TODO: use Secret
    home_assistant_token: String,
}

impl CommandExecutorConfig {
    pub fn new(home_assistant_base_url: Url, home_assistant_token: String) -> Self {
        Self {
            home_assistant_base_url,
            home_assistant_token,
        }
    }
}

pub fn execute_command(config: &CommandExecutorConfig, command: &str) -> Result<String> {
    let intent = parse_intent(command);

    match intent {
        Intent::TurnOnLight { area } => {
            let area_clone = area.clone();

            println!("Turning on lights in area: {:?}", area_clone);

            turn_on_light(config, area)?;
            let area_msg = area_clone
                .map(|a| format!(" in {}", a.replace('_', " ")))
                .unwrap_or_default();
            Ok(format!("Lights turned on{}", area_msg))
        }
        Intent::TurnOffLight { area } => {
            let area_clone = area.clone();
            turn_off_light(config, area)?;
            let area_msg = area_clone
                .map(|a| format!(" in {}", a.replace('_', " ")))
                .unwrap_or_default();
            Ok(format!("Lights turned off{}", area_msg))
        }
        Intent::GetCurrentTime => get_current_time(),
        Intent::Unknown => {
            println!("Unknown command: '{}'", command);
            Ok("Unknown command".to_string())
        }
    }
}
