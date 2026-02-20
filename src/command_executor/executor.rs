use chrono::Datelike;
use chrono::Local;
use chrono::Timelike;
use color_eyre::eyre::Result;
use pest::Parser;
use pest::iterators::Pairs;

use crate::command_executor::config::CommandExecutorConfig;
use crate::command_executor::grammar::CommandParser;
use crate::command_executor::grammar::Rule;
use crate::command_executor::services::home_assistant::{
    extract_area, turn_off_light, turn_on_light,
};
use crate::command_executor::services::timer::TimerManager;
use crate::command_executor::services::weather;
use crate::human_format::{int_to_words, words_to_int};

#[derive(Debug, PartialEq)]
enum Intent {
    TurnOnLight {
        area: Option<String>,
    },
    TurnOffLight {
        area: Option<String>,
    },
    GetCurrentTime,
    GetWeather,
    SetTimer {
        duration_secs: u64,
        name: Option<String>,
    },
    GetTimers,
    CancelTimer {
        name: Option<String>,
    },
    CancelTimerByDuration {
        duration_secs: u64,
    },
    CancelAllTimers,
    Unknown,
}

/// Parse a timer_duration rule into total seconds.
/// timer_duration contains one or more duration_segment children,
/// each with a number and a time_unit.
fn parse_timer_duration(pairs: Pairs<Rule>) -> Option<u64> {
    let mut total_secs: u64 = 0;

    for pair in pairs {
        match pair.as_rule() {
            Rule::duration_segment => {
                let inner = pair.into_inner();
                let mut number_val: Option<u64> = None;
                let mut multiplier: Option<u64> = None;

                for seg in inner {
                    match seg.as_rule() {
                        Rule::number => {
                            number_val = words_to_int(seg.as_str().trim());
                        }
                        Rule::time_unit => {
                            let unit_inner = seg.into_inner().next()?;
                            multiplier = Some(match unit_inner.as_rule() {
                                Rule::time_unit_second => 1,
                                Rule::time_unit_minute => 60,
                                Rule::time_unit_hour => 3600,
                                _ => return None,
                            });
                        }
                        _ => {} // skip whitespace
                    }
                }

                total_secs += number_val? * multiplier?;
            }
            Rule::timer_duration => {
                if let Some(secs) = parse_timer_duration(pair.into_inner()) {
                    total_secs += secs;
                }
            }
            _ => {} // skip whitespace
        }
    }

    if total_secs > 0 {
        Some(total_secs)
    } else {
        None
    }
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
        Rule::set_timer_command => {
            let inner = pairs.into_inner();
            let mut duration_secs = None;
            let mut name = None;

            for pair in inner {
                match pair.as_rule() {
                    Rule::timer_duration => {
                        duration_secs = parse_timer_duration(pair.into_inner());
                    }
                    Rule::timer_name => {
                        name = Some(pair.as_str().trim().to_string());
                    }
                    _ => {}
                }
            }

            match duration_secs {
                Some(secs) => Intent::SetTimer {
                    duration_secs: secs,
                    name,
                },
                None => Intent::Unknown,
            }
        }
        Rule::get_timers_command => Intent::GetTimers,
        Rule::cancel_all_timers_command => Intent::CancelAllTimers,
        Rule::cancel_timer_command => {
            let inner = pairs.into_inner();
            let mut duration_secs = None;
            let mut name = None;

            for pair in inner {
                match pair.as_rule() {
                    Rule::timer_duration => {
                        duration_secs = parse_timer_duration(pair.into_inner());
                    }
                    Rule::timer_name => {
                        name = Some(pair.as_str().trim().to_string());
                    }
                    _ => {}
                }
            }

            if let Some(secs) = duration_secs {
                Intent::CancelTimerByDuration {
                    duration_secs: secs,
                }
            } else {
                Intent::CancelTimer { name }
            }
        }
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

pub fn execute_command(
    config: &CommandExecutorConfig,
    timer_manager: &TimerManager,
    command: &str,
) -> Result<String> {
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
        Intent::SetTimer {
            duration_secs,
            name,
        } => Ok(timer_manager.set_timer(duration_secs, name)),
        Intent::GetTimers => Ok(timer_manager.get_timers()),
        Intent::CancelTimer { name } => match name {
            Some(n) => Ok(timer_manager.cancel_timer_by_name(&n)),
            None => Ok(timer_manager.cancel_only_timer()),
        },
        Intent::CancelTimerByDuration { duration_secs } => {
            Ok(timer_manager.cancel_timer_by_duration(duration_secs))
        }
        Intent::CancelAllTimers => Ok(timer_manager.cancel_all_timers()),
        Intent::Unknown => {
            println!("Unknown command: '{}'", command);
            Ok("Unknown command".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pest::Parser;

    #[test]
    fn test_parse_set_timer_digits() {
        let input = "set a timer for 10 minutes";
        let result = CommandParser::parse(Rule::command, input);
        println!("Parse result for '{}': {:?}", input, result);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_set_timer_words() {
        let input = "set a timer for five minutes";
        let result = CommandParser::parse(Rule::command, input);
        println!("Parse result for '{}': {:?}", input, result);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_number_digits() {
        let input = "10";
        let result = CommandParser::parse(Rule::number, input);
        println!("number parse for '{}': {:?}", input, result);
        assert!(result.is_ok(), "Failed to parse number: {:?}", result.err());
    }

    #[test]
    fn test_parse_duration_segment_digits() {
        let input = "10 minutes";
        let result = CommandParser::parse(Rule::duration_segment, input);
        println!("duration_segment parse for '{}': {:?}", input, result);
        assert!(
            result.is_ok(),
            "Failed to parse duration_segment: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_parse_timer_duration_compound() {
        let input = "one hour thirty five minutes";
        let result = CommandParser::parse(Rule::timer_duration, input);
        println!("timer_duration parse for '{}': {:?}", input, result);
        assert!(
            result.is_ok(),
            "Failed to parse timer_duration: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_intent_set_timer_digits() {
        let intent = parse_intent("set a timer for 10 minutes");
        println!("Intent: {:?}", intent);
        assert!(
            matches!(
                intent,
                Intent::SetTimer {
                    duration_secs: 600,
                    ..
                }
            ),
            "Got: {:?}",
            intent
        );
    }

    #[test]
    fn test_intent_set_timer_words() {
        let intent = parse_intent("set a timer for five minutes");
        println!("Intent: {:?}", intent);
        assert!(
            matches!(
                intent,
                Intent::SetTimer {
                    duration_secs: 300,
                    ..
                }
            ),
            "Got: {:?}",
            intent
        );
    }

    #[test]
    fn test_intent_set_timer_compound() {
        let intent = parse_intent("set a timer for one hour thirty five minutes");
        println!("Intent: {:?}", intent);
        assert!(
            matches!(
                intent,
                Intent::SetTimer {
                    duration_secs: 5700,
                    ..
                }
            ),
            "Got: {:?}",
            intent
        );
    }

    #[test]
    fn test_intent_set_timer_with_name() {
        let intent = parse_intent("set a timer for five minutes called pizza");
        println!("Intent: {:?}", intent);
        match intent {
            Intent::SetTimer {
                duration_secs,
                name,
            } => {
                assert_eq!(duration_secs, 300);
                assert_eq!(name, Some("pizza".to_string()));
            }
            _ => panic!("Got: {:?}", intent),
        }
    }

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
