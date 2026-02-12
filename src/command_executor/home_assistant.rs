use std::time::Duration;

use color_eyre::eyre::{Context, Result};
use serde_json::json;

use crate::command_executor::{CommandExecutorConfig, grammar::Rule};

pub fn extract_area(pairs: &mut pest::iterators::Pairs<'_, Rule>) -> Option<String> {
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

pub fn normalize_area_name(area: &str) -> String {
    // Convert area names from grammar to Home Assistant area IDs
    match area.trim() {
        "living room" => "living_room".to_string(),
        "bedroom" => "bedroom".to_string(),
        "hallway" => "hallway".to_string(),
        "kitchen" => "kitchen".to_string(),
        other => other.replace(' ', "_"),
    }
}

pub fn turn_on_light(config: &CommandExecutorConfig, area: Option<String>) -> Result<()> {
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

pub fn turn_off_light(config: &CommandExecutorConfig, area: Option<String>) -> Result<()> {
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
