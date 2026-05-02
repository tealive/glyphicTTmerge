use crate::paths;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::BufRead;
use std::path::Path;

#[tauri::command]
pub fn get_stats() -> Result<serde_json::Value, String> {
    let path = paths::stats_cache_path();

    if !path.exists() {
        return Ok(serde_json::json!({}));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read stats: {e}"))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse stats: {e}"))
}

#[derive(Serialize)]
pub struct LiveDailyActivity {
    pub date: String,
    #[serde(rename = "messageCount")]
    pub message_count: u32,
    #[serde(rename = "sessionCount")]
    pub session_count: u32,
    #[serde(rename = "toolCallCount")]
    pub tool_call_count: u32,
}

#[derive(Serialize)]
pub struct LiveStats {
    #[serde(rename = "dailyActivity")]
    pub daily_activity: Vec<LiveDailyActivity>,
    #[serde(rename = "totalSessions")]
    pub total_sessions: u32,
    #[serde(rename = "totalMessages")]
    pub total_messages: u32,
    #[serde(rename = "firstSessionDate")]
    pub first_session_date: String,
    #[serde(rename = "lastSessionDate")]
    pub last_session_date: String,
    #[serde(rename = "hourCounts")]
    pub hour_counts: HashMap<String, u32>,
    // Token data from cache (if available)
    #[serde(rename = "modelUsage")]
    pub model_usage: Option<serde_json::Value>,
    #[serde(rename = "dailyModelTokens")]
    pub daily_model_tokens: Option<serde_json::Value>,
    #[serde(rename = "longestSession")]
    pub longest_session: Option<serde_json::Value>,
    #[serde(rename = "lastComputedDate")]
    pub last_computed_date: String,
}

#[tauri::command]
pub fn compute_live_stats() -> Result<LiveStats, String> {
    let history_path = paths::claude_home().join("history.jsonl");

    // Modern Claude Code stores transcripts per-project. Use that source if
    // history.jsonl is absent or empty.
    let use_history_jsonl = history_path
        .metadata()
        .map(|m| m.len() > 0)
        .unwrap_or(false);

    if !use_history_jsonl {
        return compute_live_stats_from_projects();
    }

    let file = std::fs::File::open(&history_path)
        .map_err(|e| format!("failed to open history: {e}"))?;
    let reader = std::io::BufReader::new(file);

    let mut messages_by_date: HashMap<String, u32> = HashMap::new();
    let mut sessions_by_date: HashMap<String, HashSet<String>> = HashMap::new();
    let mut hour_counts: HashMap<String, u32> = HashMap::new();
    let mut all_sessions: HashSet<String> = HashSet::new();
    let mut total_messages: u32 = 0;
    let mut first_date = String::new();
    let mut last_date = String::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let timestamp = entry.get("timestamp").and_then(|t| t.as_f64()).unwrap_or(0.0);
        let session_id = entry
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        if timestamp == 0.0 {
            continue;
        }

        // Convert millisecond timestamp to date
        let secs = (timestamp / 1000.0) as i64;
        let dt = chrono_lite_date(secs);
        let date = dt.0.clone();
        let hour = dt.1;

        *messages_by_date.entry(date.clone()).or_insert(0) += 1;
        sessions_by_date
            .entry(date.clone())
            .or_default()
            .insert(session_id.clone());
        *hour_counts.entry(hour.to_string()).or_insert(0) += 1;
        all_sessions.insert(session_id);
        total_messages += 1;

        if first_date.is_empty() || date < first_date {
            first_date = date.clone();
        }
        if last_date.is_empty() || date > last_date {
            last_date = date.clone();
        }
    }

    // Build daily activity sorted by date
    let mut dates: Vec<String> = messages_by_date.keys().cloned().collect();
    dates.sort();

    let daily_activity: Vec<LiveDailyActivity> = dates
        .iter()
        .map(|date| LiveDailyActivity {
            date: date.clone(),
            message_count: *messages_by_date.get(date).unwrap_or(&0),
            session_count: sessions_by_date
                .get(date)
                .map(|s| s.len() as u32)
                .unwrap_or(0),
            tool_call_count: 0, // history.jsonl doesn't track tool calls
        })
        .collect();

    // Read cache for token data (we can't compute this from history.jsonl)
    let cache = get_stats().ok();
    let model_usage = cache
        .as_ref()
        .and_then(|c| c.get("modelUsage").cloned());
    let daily_model_tokens = cache
        .as_ref()
        .and_then(|c| c.get("dailyModelTokens").cloned());
    let longest_session = cache
        .as_ref()
        .and_then(|c| c.get("longestSession").cloned());

    // Merge tool call counts from cache where available
    let mut daily_activity = daily_activity;
    if let Some(cached_stats) = &cache {
        if let Some(cached_daily) = cached_stats.get("dailyActivity").and_then(|d| d.as_array()) {
            let cached_map: HashMap<String, u32> = cached_daily
                .iter()
                .filter_map(|d| {
                    let date = d.get("date")?.as_str()?.to_string();
                    let tool_calls = d.get("toolCallCount")?.as_u64()? as u32;
                    Some((date, tool_calls))
                })
                .collect();

            for activity in &mut daily_activity {
                if let Some(&tool_calls) = cached_map.get(&activity.date) {
                    activity.tool_call_count = tool_calls;
                }
            }
        }
    }

    let today = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        chrono_lite_date(now).0
    };

    Ok(LiveStats {
        daily_activity,
        total_sessions: all_sessions.len() as u32,
        total_messages,
        first_session_date: first_date,
        last_session_date: last_date,
        hour_counts,
        model_usage,
        daily_model_tokens,
        longest_session,
        last_computed_date: today,
    })
}

/// Aggregate stats by walking ~/.claude/projects/<project>/<session>.jsonl.
///
/// Per-project transcripts use ISO 8601 timestamps (e.g. "2026-04-26T22:45:47.406Z")
/// and have a richer schema than legacy history.jsonl. Each line has a `type`
/// field; we count user/assistant turns as "messages", count tool_use blocks
/// inside assistant message content as "tool calls", and use the .jsonl filename
/// (sans extension) as the session id.
fn compute_live_stats_from_projects() -> Result<LiveStats, String> {
    let projects_dir = paths::projects_dir();
    if !projects_dir.exists() {
        return Err("no ~/.claude/projects directory found".to_string());
    }

    let mut messages_by_date: HashMap<String, u32> = HashMap::new();
    let mut sessions_by_date: HashMap<String, HashSet<String>> = HashMap::new();
    let mut tool_calls_by_date: HashMap<String, u32> = HashMap::new();
    let mut hour_counts: HashMap<String, u32> = HashMap::new();
    let mut all_sessions: HashSet<String> = HashSet::new();
    let mut total_messages: u32 = 0;
    let mut first_date = String::new();
    let mut last_date = String::new();

    let project_entries = fs::read_dir(&projects_dir)
        .map_err(|e| format!("failed to read projects dir: {e}"))?;

    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        // Read top-level <session>.jsonl files in this project. Skip nested
        // subagents/ so we don't double-count sub-agent traces.
        let session_entries = match fs::read_dir(&project_path) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for session_entry in session_entries.flatten() {
            let path = session_entry.path();
            if !is_top_level_jsonl(&path) {
                continue;
            }
            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if session_id.is_empty() {
                continue;
            }
            scan_transcript(
                &path,
                &session_id,
                &mut messages_by_date,
                &mut sessions_by_date,
                &mut tool_calls_by_date,
                &mut hour_counts,
                &mut all_sessions,
                &mut total_messages,
                &mut first_date,
                &mut last_date,
            );
        }
    }

    let mut dates: Vec<String> = messages_by_date.keys().cloned().collect();
    dates.sort();

    let daily_activity: Vec<LiveDailyActivity> = dates
        .iter()
        .map(|date| LiveDailyActivity {
            date: date.clone(),
            message_count: *messages_by_date.get(date).unwrap_or(&0),
            session_count: sessions_by_date
                .get(date)
                .map(|s| s.len() as u32)
                .unwrap_or(0),
            tool_call_count: *tool_calls_by_date.get(date).unwrap_or(&0),
        })
        .collect();

    // Token usage data is maintained separately by the existing stats cache
    // writer (Claude Code itself); merge that in if present.
    let cache = get_stats().ok();
    let model_usage = cache.as_ref().and_then(|c| c.get("modelUsage").cloned());
    let daily_model_tokens = cache
        .as_ref()
        .and_then(|c| c.get("dailyModelTokens").cloned());
    let longest_session = cache
        .as_ref()
        .and_then(|c| c.get("longestSession").cloned());

    let today = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        chrono_lite_date(now).0
    };

    Ok(LiveStats {
        daily_activity,
        total_sessions: all_sessions.len() as u32,
        total_messages,
        first_session_date: first_date,
        last_session_date: last_date,
        hour_counts,
        model_usage,
        daily_model_tokens,
        longest_session,
        last_computed_date: today,
    })
}

fn is_top_level_jsonl(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jsonl")
}

#[allow(clippy::too_many_arguments)]
fn scan_transcript(
    path: &Path,
    session_id: &str,
    messages_by_date: &mut HashMap<String, u32>,
    sessions_by_date: &mut HashMap<String, HashSet<String>>,
    tool_calls_by_date: &mut HashMap<String, u32>,
    hour_counts: &mut HashMap<String, u32>,
    all_sessions: &mut HashSet<String>,
    total_messages: &mut u32,
    first_date: &mut String,
    last_date: &mut String,
) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only conversational turns count as messages.
        let msg_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type != "user" && msg_type != "assistant" {
            continue;
        }

        let timestamp = match entry.get("timestamp").and_then(|t| t.as_str()) {
            Some(t) if t.len() >= 13 => t,
            _ => continue,
        };

        // ISO 8601: "YYYY-MM-DDTHH:MM:SS.sssZ" — slice without parsing the full datetime.
        let date = &timestamp[..10];
        let hour: u32 = timestamp[11..13].parse().unwrap_or(0);

        *messages_by_date.entry(date.to_string()).or_insert(0) += 1;
        sessions_by_date
            .entry(date.to_string())
            .or_default()
            .insert(session_id.to_string());
        *hour_counts.entry(hour.to_string()).or_insert(0) += 1;
        all_sessions.insert(session_id.to_string());
        *total_messages += 1;

        if first_date.is_empty() || date < first_date.as_str() {
            *first_date = date.to_string();
        }
        if last_date.is_empty() || date > last_date.as_str() {
            *last_date = date.to_string();
        }

        // Count tool_use content blocks inside assistant messages.
        if msg_type == "assistant" {
            if let Some(blocks) = entry
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                let tools = blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                    .count() as u32;
                if tools > 0 {
                    *tool_calls_by_date.entry(date.to_string()).or_insert(0) += tools;
                }
            }
        }
    }
}

/// Simple date extraction from unix timestamp without chrono crate
fn chrono_lite_date(secs: i64) -> (String, u32) {
    // Convert unix timestamp to date components
    let days = secs / 86400;
    let hour = ((secs % 86400) / 3600) as u32;

    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (format!("{y:04}-{m:02}-{d:02}"), hour)
}
