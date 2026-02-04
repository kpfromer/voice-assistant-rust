use std::time::Duration;

use color_eyre::eyre::{Context, Result};
use pest::Parser;
use pest_derive::Parser;
use serde_json::json;
use url::Url;

#[derive(Parser)]
#[grammar = "command_grammar.pest"]
struct CommandParser;

#[derive(Debug)]
enum Intent {
    TurnOnLight { area: Option<String> },
    TurnOffLight { area: Option<String> },
    Unknown,
}

fn parse_intent(command: &str) -> Intent {
    let command_lower = command.to_lowercase();
    let pairs = match CommandParser::parse(Rule::light_command, &command_lower) {
        Ok(mut pairs) => pairs.next().unwrap(),
        Err(e) => {
            eprintln!("Parse error: {:?}", e);
            return Intent::Unknown;
        }
    };

    let (is_turn_on, area) = match pairs.as_rule() {
        Rule::turn_on_lights_command => {
            let mut inner = pairs.into_inner();
            inner.next(); // Skip turn_on
            let area = extract_area(&mut inner);
            (true, area)
        }
        Rule::turn_off_lights_command => {
            let mut inner = pairs.into_inner();
            inner.next(); // Skip turn_off
            let area = extract_area(&mut inner);
            (false, area)
        }
        _ => return Intent::Unknown,
    };

    if is_turn_on {
        Intent::TurnOnLight { area }
    } else {
        Intent::TurnOffLight { area }
    }
}

fn extract_area(pairs: &mut pest::iterators::Pairs<'_, Rule>) -> Option<String> {
    let Some(pair) = pairs.next() else {
        return None;
    };

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
        Intent::Unknown => {
            println!("Unknown command: '{}'", command);
            Ok("I can't do that".to_string())
        }
    }
}
