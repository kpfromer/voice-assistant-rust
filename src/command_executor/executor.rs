use chrono::Datelike;
use chrono::Local;
use chrono::Timelike;
use color_eyre::eyre::Result;
use pest::Parser;

use crate::command_executor::config::CommandExecutorConfig;
use crate::command_executor::grammar::CommandParser;
use crate::command_executor::grammar::Rule;
use crate::command_executor::services::home_assistant::{
    extract_area, turn_off_light, turn_on_light,
};
use crate::command_executor::services::weather;
use crate::human_format::int_to_words;

#[derive(Debug, PartialEq)]
enum Intent {
    TurnOnLight { area: Option<String> },
    TurnOffLight { area: Option<String> },
    GetCurrentTime,
    GetWeather,
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
        Rule::whats_the_weather_command => Intent::GetWeather,
        _ => return Intent::Unknown,
    }
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

fn get_weather(config: &CommandExecutorConfig) -> Result<String> {
    match (config.weather_latitude, config.weather_longitude) {
        (Some(latitude), Some(longitude)) => {
            let weather = weather::get_weather(latitude, longitude)?;
            Ok(weather)
        }
        (Some(_), None) => Ok("Weather longitude is not set".to_string()),
        (None, Some(_)) => Ok("Weather latitude is not set".to_string()),
        (None, None) => Ok(
            "Weather service is not configured. Please set weather latitude and longitude"
                .to_string(),
        ),
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
        Intent::GetWeather => get_weather(config),
        Intent::Unknown => {
            println!("Unknown command: '{}'", command);
            Ok("Unknown command".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Weather commands
    #[test]
    fn parse_whats_the_weather() {
        assert_eq!(parse_intent("whats the weather"), Intent::GetWeather);
    }

    #[test]
    fn parse_what_is_the_weather() {
        assert_eq!(parse_intent("what is the weather"), Intent::GetWeather);
    }

    #[test]
    fn parse_whats_the_weather_like() {
        assert_eq!(parse_intent("whats the weather like"), Intent::GetWeather);
    }

    #[test]
    fn parse_what_is_the_weather_like() {
        assert_eq!(parse_intent("what is the weather like"), Intent::GetWeather);
    }

    #[test]
    fn parse_weather_case_insensitive() {
        assert_eq!(parse_intent("Whats The Weather"), Intent::GetWeather);
    }

    // Time commands
    #[test]
    fn parse_what_time_is_it() {
        assert_eq!(parse_intent("what time is it"), Intent::GetCurrentTime);
    }

    #[test]
    fn parse_what_is_the_time() {
        assert_eq!(parse_intent("what is the time"), Intent::GetCurrentTime);
    }

    #[test]
    fn parse_time_case_insensitive() {
        assert_eq!(parse_intent("What Time Is It"), Intent::GetCurrentTime);
    }

    // Turn on lights commands
    #[test]
    fn parse_turn_on_lights() {
        assert_eq!(
            parse_intent("turn on lights"),
            Intent::TurnOnLight { area: None }
        );
    }

    #[test]
    fn parse_turn_on_light_singular() {
        assert_eq!(
            parse_intent("turn on light"),
            Intent::TurnOnLight { area: None }
        );
    }

    #[test]
    fn parse_turn_on_all_lights() {
        assert_eq!(
            parse_intent("turn on all lights"),
            Intent::TurnOnLight { area: None }
        );
    }

    #[test]
    fn parse_turn_on_lights_in_bedroom() {
        assert_eq!(
            parse_intent("turn on lights in the bedroom"),
            Intent::TurnOnLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_lights_in_living_room() {
        assert_eq!(
            parse_intent("turn on lights in the living room"),
            Intent::TurnOnLight {
                area: Some("living_room".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_lights_in_kitchen() {
        assert_eq!(
            parse_intent("turn on lights in the kitchen"),
            Intent::TurnOnLight {
                area: Some("kitchen".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_lights_in_hallway() {
        assert_eq!(
            parse_intent("turn on lights in the hallway"),
            Intent::TurnOnLight {
                area: Some("hallway".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_lights_for_area() {
        assert_eq!(
            parse_intent("turn on lights for the bedroom"),
            Intent::TurnOnLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_lights_area_without_the() {
        assert_eq!(
            parse_intent("turn on lights in bedroom"),
            Intent::TurnOnLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_area_before_lights() {
        assert_eq!(
            parse_intent("turn on bedroom lights"),
            Intent::TurnOnLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_on_living_room_lights() {
        assert_eq!(
            parse_intent("turn on living room lights"),
            Intent::TurnOnLight {
                area: Some("living_room".to_string())
            }
        );
    }

    // Turn off lights commands
    #[test]
    fn parse_turn_off_lights() {
        assert_eq!(
            parse_intent("turn off lights"),
            Intent::TurnOffLight { area: None }
        );
    }

    #[test]
    fn parse_turn_off_all_lights() {
        assert_eq!(
            parse_intent("turn off all lights"),
            Intent::TurnOffLight { area: None }
        );
    }

    #[test]
    fn parse_turn_off_lights_in_bedroom() {
        assert_eq!(
            parse_intent("turn off lights in the bedroom"),
            Intent::TurnOffLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    #[test]
    fn parse_turn_off_bedroom_lights() {
        assert_eq!(
            parse_intent("turn off bedroom lights"),
            Intent::TurnOffLight {
                area: Some("bedroom".to_string())
            }
        );
    }

    // Unknown commands
    #[test]
    fn parse_unknown_command() {
        assert_eq!(parse_intent("hello world"), Intent::Unknown);
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(parse_intent(""), Intent::Unknown);
    }

    #[test]
    fn parse_gibberish() {
        assert_eq!(parse_intent("asdfghjkl"), Intent::Unknown);
    }

    // Whitespace handling
    #[test]
    fn parse_with_trailing_whitespace() {
        assert_eq!(parse_intent("whats the weather  "), Intent::GetWeather);
    }
}
