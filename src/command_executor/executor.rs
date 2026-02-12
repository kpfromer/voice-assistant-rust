use chrono::Datelike;
use chrono::Local;
use chrono::Timelike;
use color_eyre::eyre::Result;
use pest::Parser;

use crate::command_executor::config::CommandExecutorConfig;
use crate::command_executor::grammar::CommandParser;
use crate::command_executor::grammar::Rule;
use crate::command_executor::home_assistant::extract_area;
use crate::command_executor::home_assistant::turn_off_light;
use crate::command_executor::home_assistant::turn_on_light;
use crate::human_format::int_to_words;

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
