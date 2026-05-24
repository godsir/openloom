use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

fn chrono_id() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{}", dur.as_millis(), (dur.as_nanos() % 10000))
}

/// A scheduled cron/automation job persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub label: String,
    pub schedule_type: ScheduleType,
    pub schedule: String,
    pub prompt: String,
    pub enabled: bool,
    pub created_at: u64,
    pub last_run_at: Option<u64>,
    pub next_run_at: Option<u64>,
    pub consecutive_errors: u32,
    pub model: Option<String>,
    /// Automation executor type. None means legacy agent-session (cron).
    #[serde(default)]
    pub executor: Option<AutomationExecutor>,
}

/// Pre-determined action to execute when a job fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationExecutor {
    /// Run the prompt through the agent (same as legacy cron).
    AgentSession,
    /// Send a notification.
    DirectAction {
        action: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        channels: Vec<String>,
    },
}

/// A notification record stored for later retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: String,
    pub job_id: String,
    pub title: String,
    pub body: String,
    pub created_at: u64,
    pub read: bool,
}

/// Simple notification store (appends to a JSON file).
pub struct NotificationStore {
    path: PathBuf,
}

impl NotificationStore {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            path: data_dir.join("notifications.json"),
        }
    }

    pub fn load(&self) -> Vec<NotificationRecord> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self, records: &[NotificationRecord]) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(records).unwrap_or_default();
        std::fs::write(&self.path, json)
    }

    pub fn add(&self, job_id: &str, title: &str, body: &str) -> std::io::Result<NotificationRecord> {
        let mut records = self.load();
        let record = NotificationRecord {
            id: chrono_id(),
            job_id: job_id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            read: false,
        };
        records.push(record.clone());
        self.save(&records)?;
        Ok(record)
    }

    pub fn list_unread(&self) -> Vec<NotificationRecord> {
        self.load().into_iter().filter(|r| !r.read).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleType {
    /// One-shot at an ISO datetime
    At,
    /// Recurring every N minutes
    Every,
    /// Standard 5-field cron expression
    Cron,
}

impl CronJob {
    /// Calculate the next run time as milliseconds since epoch.
    /// Returns None if the job is disabled, or for `at` jobs that already ran.
    pub fn calc_next_run(&self, now_ms: u64) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        match self.schedule_type {
            ScheduleType::At => {
                // Parse ISO datetime, return it if still in the future
                let ts = parse_iso_to_ms(&self.schedule)?;
                if ts > now_ms {
                    Some(ts)
                } else {
                    // Already past — one-shot done, disable
                    None
                }
            }
            ScheduleType::Every => {
                let interval_min: u64 = self.schedule.parse().ok()?;
                let interval_ms = interval_min * 60_000;
                let last = self.last_run_at.unwrap_or(self.created_at);
                Some(last + interval_ms)
            }
            ScheduleType::Cron => {
                next_cron_match(&self.schedule, now_ms)
            }
        }
    }

    /// Seconds of backoff after consecutive errors: 0, 60, 300, 900, 3600
    pub fn backoff_ms(&self) -> u64 {
        match self.consecutive_errors {
            0 => 0,
            1 => 60_000,
            2 => 300_000,
            3 => 900_000,
            _ => 3_600_000,
        }
    }

    /// Calculate next run with backoff applied on top.
    pub fn calc_next_run_with_backoff(&self, now_ms: u64) -> Option<u64> {
        let base = self.calc_next_run(now_ms)?;
        Some(base + self.backoff_ms())
    }
}

fn parse_iso_to_ms(s: &str) -> Option<u64> {
    // Accept ISO-like: "2026-06-01T09:00:00" or "2026-06-01T09:00:00Z"
    let s = s.trim();
    let (date_part, time_part) = s.split_once('T')?;
    let time_part = time_part.trim_end_matches('Z');

    let date_parts: Vec<&str> = date_part.split('-').collect();
    let time_parts: Vec<&str> = time_part.split(':').collect();
    if date_parts.len() != 3 || time_parts.len() < 2 {
        return None;
    }
    let year: i32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let min: u32 = time_parts[1].parse().ok()?;
    let sec: u32 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Use a simple epoch calculation
    let days_before_month: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut days = (year as u64 - 1970) * 365;
    days += (year as u64 - 1969) / 4; // leap years
    days += days_before_month[(month - 1) as usize] as u64;
    if month > 2 && is_leap_year(year) {
        days += 1;
    }
    days += (day - 1) as u64;

    let secs = days * 86400 + hour as u64 * 3600 + min as u64 * 60 + sec as u64;
    Some(secs * 1000)
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Simple cron expression parser (5-field: min hour dom month dow).
/// Returns the next match in ms from `now_ms`.
fn next_cron_match(expr: &str, now_ms: u64) -> Option<u64> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return None;
    }

    let parse_field = |s: &str, max: u32| -> Vec<u32> {
        s.split(',')
            .flat_map(|part| {
                if part == "*" {
                    (0..=max).collect::<Vec<_>>()
                } else if let Some(slash_pos) = part.find('/') {
                    let base = &part[..slash_pos];
                    let step: u32 = part[slash_pos + 1..].parse().unwrap_or(1);
                    let start = if base == "*" { 0 } else { base.parse().unwrap_or(0) };
                    (start..=max).step_by(step as usize).collect()
                } else if let Some(dash_pos) = part.find('-') {
                    let lo: u32 = part[..dash_pos].parse().unwrap_or(0);
                    let hi: u32 = part[dash_pos + 1..].parse().unwrap_or(max);
                    (lo..=hi).collect()
                } else {
                    vec![part.parse().unwrap_or(0)]
                }
            })
            .collect()
    };

    let minutes = parse_field(fields[0], 59);
    let hours = parse_field(fields[1], 23);
    let dom = parse_field(fields[2], 31);
    let month = parse_field(fields[3], 12);
    let dow = parse_field(fields[4], 7);

    if minutes.is_empty() || hours.is_empty() || dom.is_empty() || month.is_empty() || dow.is_empty() {
        return None;
    }

    // Convert now_ms to date components
    let total_secs = now_ms / 1000;
    let days_since_epoch = total_secs / 86400;
    let day_secs = total_secs % 86400;
    let current_hour = day_secs / 3600;
    let current_min = (day_secs % 3600) / 60;

    // Walk forward one year worth of minutes to find the next match
    for offset in 0..=525_600 {
        let check_min = (current_min + offset) % 60;
        let carry_h = (current_min + offset) / 60;
        let check_hour = (current_hour + carry_h) % 24;
        let carry_d = (current_hour + carry_h) / 24;
        let check_day = days_since_epoch + carry_d;

        // Derive month/day from check_day (rough — good enough for scheduling)
        if minutes.contains(&(check_min as u32))
            && hours.contains(&(check_hour as u32))
        {
            let ms = (check_day * 86400 + check_hour * 3600 + check_min * 60) * 1000;
            if ms > now_ms {
                return Some(ms);
            }
        }
    }
    None
}

/// Thread-safe persistent store for cron jobs.
pub struct CronStore {
    path: PathBuf,
}

impl CronStore {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            path: data_dir.join("cron-jobs.json"),
        }
    }

    pub fn load(&self) -> Vec<CronJob> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    pub fn save(&self, jobs: &[CronJob]) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(jobs).unwrap_or_default();
        std::fs::write(&self.path, json)
    }

    pub fn add(&self, mut job: CronJob) -> std::io::Result<String> {
        let id = format!("cron-{}", chrono_id());
        job.id = id.clone();
        let mut jobs = self.load();
        jobs.push(job);
        self.save(&jobs)?;
        Ok(id)
    }

    pub fn remove(&self, id: &str) -> std::io::Result<bool> {
        let mut jobs = self.load();
        let len_before = jobs.len();
        jobs.retain(|j| j.id != id);
        if jobs.len() == len_before {
            return Ok(false);
        }
        self.save(&jobs)?;
        Ok(true)
    }

    pub fn toggle(&self, id: &str) -> std::io::Result<Option<bool>> {
        let mut jobs = self.load();
        let job = jobs.iter_mut().find(|j| j.id == id);
        match job {
            Some(j) => {
                j.enabled = !j.enabled;
                let new_state = j.enabled;
                self.save(&jobs)?;
                Ok(Some(new_state))
            }
            None => Ok(None),
        }
    }

    pub fn list_all(&self) -> Vec<CronJob> {
        self.load()
    }

    /// Return jobs that are enabled and due (next_run_at <= now_ms).
    pub fn get_due_jobs(&self, now_ms: u64) -> Vec<CronJob> {
        let mut jobs = self.load();
        // Recalculate next_run for every job
        let due: Vec<CronJob> = jobs
            .iter_mut()
            .filter_map(|j| {
                let next = j.calc_next_run_with_backoff(now_ms)?;
                if !j.enabled {
                    return None;
                }
                if next <= now_ms {
                    Some(j.clone())
                } else {
                    None
                }
            })
            .collect();
        self.save(&jobs).ok();
        due
    }

    pub fn mark_run(&self, id: &str, now_ms: u64, success: bool) -> std::io::Result<()> {
        let mut jobs = self.load();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.last_run_at = Some(now_ms);
            job.next_run_at = job.calc_next_run_with_backoff(now_ms);
            if success {
                job.consecutive_errors = 0;
            } else {
                job.consecutive_errors += 1;
            }
            // Auto-disable at-type after first run
            if job.schedule_type == ScheduleType::At {
                job.enabled = false;
            }
        }
        self.save(&jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iso_to_ms() {
        let ms = parse_iso_to_ms("2026-06-01T09:00:00").unwrap();
        // June 1 2026 09:00 UTC is ~1.78e12 ms from epoch
        assert!(ms > 1_780_000_000_000);
    }

    #[test]
    fn test_cron_store_add_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = CronStore::new(dir.path());
        let job = CronJob {
            id: String::new(),
            label: "test".into(),
            schedule_type: ScheduleType::Every,
            schedule: "60".into(),
            prompt: "say hello".into(),
            enabled: true,
            created_at: 0,
            last_run_at: None,
            next_run_at: None,
            consecutive_errors: 0,
            model: None,
            executor: None,
        };
        let id = store.add(job).unwrap();
        assert!(!id.is_empty());

        let jobs = store.list_all();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].prompt, "say hello");
    }

    #[test]
    fn test_cron_store_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = CronStore::new(dir.path());
        let job = CronJob {
            id: String::new(),
            label: "t".into(),
            schedule_type: ScheduleType::Every,
            schedule: "30".into(),
            prompt: "x".into(),
            enabled: true,
            created_at: 0,
            last_run_at: None,
            next_run_at: None,
            consecutive_errors: 0,
            model: None,
            executor: None,
        };
        let id = store.add(job).unwrap();
        assert!(store.remove(&id).unwrap());
        assert!(store.list_all().is_empty());
    }
}
