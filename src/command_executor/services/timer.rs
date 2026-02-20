use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::human_format::int_to_words;

pub struct TimerEvent {
    pub name: Option<String>,
}

struct TimerInfo {
    pub id: u64,
    pub name: Option<String>,
    pub original_duration_secs: u64,
    pub started_at: Instant,
    pub cancelled: Arc<AtomicBool>,
}

pub struct TimerManager {
    timers: Arc<Mutex<HashMap<u64, TimerInfo>>>,
    next_id: Arc<Mutex<u64>>,
    event_sender: mpsc::Sender<TimerEvent>,
}

impl TimerManager {
    pub fn new(event_sender: mpsc::Sender<TimerEvent>) -> Self {
        Self {
            timers: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
            event_sender,
        }
    }

    pub fn set_timer(&self, duration_secs: u64, name: Option<String>) -> String {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let cancelled = Arc::new(AtomicBool::new(false));

        let info = TimerInfo {
            id,
            name: name.clone(),
            original_duration_secs: duration_secs,
            started_at: Instant::now(),
            cancelled: cancelled.clone(),
        };

        self.timers.lock().unwrap().insert(id, info);

        let timers = self.timers.clone();
        let sender = self.event_sender.clone();
        let timer_name = name.clone();

        thread::spawn(move || {
            // Sleep in small increments so we can check for cancellation
            let total = Duration::from_secs(duration_secs);
            let start = Instant::now();
            while start.elapsed() < total {
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }

            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            // Remove from active timers
            timers.lock().unwrap().remove(&id);

            // Send event
            let _ = sender.send(TimerEvent {
                name: timer_name,
            });
        });

        let duration_str = format_duration_human(duration_secs);
        match name {
            Some(n) => format!("Timer {} set for {}", n, duration_str),
            None => format!("Timer set for {}", duration_str),
        }
    }

    pub fn get_timers(&self) -> String {
        let timers = self.timers.lock().unwrap();
        if timers.is_empty() {
            return "No timers are set".to_string();
        }

        let mut lines = Vec::new();
        for timer in timers.values() {
            let elapsed = timer.started_at.elapsed().as_secs();
            let remaining = timer.original_duration_secs.saturating_sub(elapsed);
            let remaining_str = format_duration_human(remaining);
            let name_str = timer
                .name
                .as_ref()
                .map(|n| format!("{}: ", n))
                .unwrap_or_default();
            lines.push(format!("{}{} remaining", name_str, remaining_str));
        }

        lines.join(". ")
    }

    pub fn cancel_timer_by_name(&self, name: &str) -> String {
        let mut timers = self.timers.lock().unwrap();
        let name_lower = name.to_lowercase();

        let id = timers
            .values()
            .find(|t| {
                t.name
                    .as_ref()
                    .map(|n| n.to_lowercase() == name_lower)
                    .unwrap_or(false)
            })
            .map(|t| t.id);

        if let Some(id) = id {
            if let Some(timer) = timers.remove(&id) {
                timer.cancelled.store(true, Ordering::Relaxed);
                return format!(
                    "Cancelled timer {}",
                    timer.name.unwrap_or_else(|| format_duration_human(
                        timer.original_duration_secs
                    ))
                );
            }
        }

        // If no name match, try cancelling the only timer if there's just one
        if timers.len() == 1 {
            let id = *timers.keys().next().unwrap();
            let timer = timers.remove(&id).unwrap();
            timer.cancelled.store(true, Ordering::Relaxed);
            return format!(
                "Cancelled timer {}",
                timer
                    .name
                    .unwrap_or_else(|| format_duration_human(timer.original_duration_secs))
            );
        }

        format!("No timer found with name {}", name)
    }

    pub fn cancel_timer_by_duration(&self, duration_secs: u64) -> String {
        let mut timers = self.timers.lock().unwrap();

        let id = timers
            .values()
            .find(|t| t.original_duration_secs == duration_secs)
            .map(|t| t.id);

        if let Some(id) = id {
            if let Some(timer) = timers.remove(&id) {
                timer.cancelled.store(true, Ordering::Relaxed);
                let label = timer
                    .name
                    .unwrap_or_else(|| format_duration_human(timer.original_duration_secs));
                return format!("Cancelled timer {}", label);
            }
        }

        format!(
            "No timer found for {}",
            format_duration_human(duration_secs)
        )
    }

    pub fn cancel_all_timers(&self) -> String {
        let mut timers = self.timers.lock().unwrap();
        if timers.is_empty() {
            return "No timers to cancel".to_string();
        }
        let count = timers.len();
        for timer in timers.values() {
            timer.cancelled.store(true, Ordering::Relaxed);
        }
        timers.clear();
        format!("Cancelled {} timer{}", count, if count == 1 { "" } else { "s" })
    }

    pub fn cancel_only_timer(&self) -> String {
        let mut timers = self.timers.lock().unwrap();
        if timers.len() == 1 {
            let id = *timers.keys().next().unwrap();
            let timer = timers.remove(&id).unwrap();
            timer.cancelled.store(true, Ordering::Relaxed);
            let label = timer
                .name
                .unwrap_or_else(|| format_duration_human(timer.original_duration_secs));
            format!("Cancelled timer {}", label)
        } else if timers.is_empty() {
            "No timers to cancel".to_string()
        } else {
            "Multiple timers are set. Please specify which timer to cancel.".to_string()
        }
    }
}

fn format_duration_human(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!(
            "{} hour{}",
            int_to_words(hours as i32),
            if hours == 1 { "" } else { "s" }
        ));
    }
    if minutes > 0 {
        parts.push(format!(
            "{} minute{}",
            int_to_words(minutes as i32),
            if minutes == 1 { "" } else { "s" }
        ));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!(
            "{} second{}",
            int_to_words(seconds as i32),
            if seconds == 1 { "" } else { "s" }
        ));
    }

    parts.join(" and ")
}
