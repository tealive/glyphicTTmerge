//! Tier-1 Insights: pure-local aggregate statistics over ~/.claude/.
//!
//! Reads per-project transcripts to build a richer summary than the Dashboard:
//! top projects by activity, project clusters by ACTION-* prefix, tool-call
//! frequencies, and counts of plans / scheduled tasks / memory files.

use crate::paths;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;

#[derive(Serialize)]
pub struct InsightsSummary {
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    #[serde(rename = "totalMessages")]
    pub total_messages: u32,
    #[serde(rename = "totalSessions")]
    pub total_sessions: u32,
    #[serde(rename = "totalToolCalls")]
    pub total_tool_calls: u32,
    #[serde(rename = "activeDays")]
    pub active_days: u32,
    #[serde(rename = "firstDate")]
    pub first_date: String,
    #[serde(rename = "lastDate")]
    pub last_date: String,
    #[serde(rename = "topProjects")]
    pub top_projects: Vec<ProjectStat>,
    #[serde(rename = "projectClusters")]
    pub project_clusters: Vec<ClusterStat>,
    #[serde(rename = "toolFrequencies")]
    pub tool_frequencies: Vec<ToolStat>,
    #[serde(rename = "plansCount")]
    pub plans_count: u32,
    #[serde(rename = "scheduledTasksCount")]
    pub scheduled_tasks_count: u32,
    #[serde(rename = "memoryFilesCount")]
    pub memory_files_count: u32,
}

#[derive(Serialize)]
pub struct ProjectStat {
    pub slug: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "messageCount")]
    pub message_count: u32,
    #[serde(rename = "sessionCount")]
    pub session_count: u32,
    #[serde(rename = "toolCallCount")]
    pub tool_call_count: u32,
    #[serde(rename = "lastActivity")]
    pub last_activity: String,
}

#[derive(Serialize)]
pub struct ClusterStat {
    pub key: String,
    #[serde(rename = "projectCount")]
    pub project_count: u32,
    #[serde(rename = "messageCount")]
    pub message_count: u32,
}

#[derive(Serialize)]
pub struct ToolStat {
    pub name: String,
    pub count: u32,
}

#[tauri::command]
pub fn compute_insights() -> Result<InsightsSummary, String> {
    let projects_dir = paths::projects_dir();
    if !projects_dir.exists() {
        return Err("no ~/.claude/projects directory found".to_string());
    }

    let mut total_messages: u32 = 0;
    let mut total_tool_calls: u32 = 0;
    let mut sessions = std::collections::HashSet::<String>::new();
    let mut active_days = std::collections::HashSet::<String>::new();
    let mut first_date = String::new();
    let mut last_date = String::new();
    let mut tool_freqs: HashMap<String, u32> = HashMap::new();
    let mut per_project: HashMap<String, ProjectAccumulator> = HashMap::new();

    let project_entries = fs::read_dir(&projects_dir)
        .map_err(|e| format!("failed to read projects dir: {e}"))?;

    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let slug = match project_path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let session_entries = match fs::read_dir(&project_path) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for session_entry in session_entries.flatten() {
            let path = session_entry.path();
            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
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

            let acc = per_project
                .entry(slug.clone())
                .or_insert_with(ProjectAccumulator::default);

            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
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
                let msg_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if msg_type != "user" && msg_type != "assistant" {
                    continue;
                }
                let timestamp = match entry.get("timestamp").and_then(|t| t.as_str()) {
                    Some(t) if t.len() >= 13 => t,
                    _ => continue,
                };
                let date = &timestamp[..10];

                total_messages += 1;
                acc.message_count += 1;
                sessions.insert(session_id.clone());
                acc.sessions.insert(session_id.clone());
                active_days.insert(date.to_string());
                if first_date.is_empty() || date < first_date.as_str() {
                    first_date = date.to_string();
                }
                if last_date.is_empty() || date > last_date.as_str() {
                    last_date = date.to_string();
                }
                if acc.last_activity.as_str() < date {
                    acc.last_activity = date.to_string();
                }

                if msg_type == "assistant" {
                    if let Some(blocks) = entry
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                    {
                        for b in blocks {
                            if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                total_tool_calls += 1;
                                acc.tool_call_count += 1;
                                if let Some(name) = b.get("name").and_then(|n| n.as_str()) {
                                    *tool_freqs.entry(name.to_string()).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Top projects (max 15)
    let mut top_projects: Vec<ProjectStat> = per_project
        .into_iter()
        .map(|(slug, a)| ProjectStat {
            display_name: prettify_slug(&slug),
            slug,
            message_count: a.message_count,
            session_count: a.sessions.len() as u32,
            tool_call_count: a.tool_call_count,
            last_activity: a.last_activity,
        })
        .collect();
    top_projects.sort_by(|a, b| b.message_count.cmp(&a.message_count));
    let cluster_input: Vec<(String, u32)> = top_projects
        .iter()
        .map(|p| (p.slug.clone(), p.message_count))
        .collect();
    top_projects.truncate(15);

    let project_clusters = build_clusters(&cluster_input);

    // Top tools (max 20)
    let mut tool_frequencies: Vec<ToolStat> = tool_freqs
        .into_iter()
        .map(|(name, count)| ToolStat { name, count })
        .collect();
    tool_frequencies.sort_by(|a, b| b.count.cmp(&a.count));
    tool_frequencies.truncate(20);

    let plans_count = count_files(paths::claude_home().join("plans"));
    let scheduled_tasks_count = count_dir_entries(paths::claude_home().join("scheduled-tasks"));
    let memory_files_count = count_memory_files(&projects_dir);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let generated_at = format_date(now);

    Ok(InsightsSummary {
        generated_at,
        total_messages,
        total_sessions: sessions.len() as u32,
        total_tool_calls,
        active_days: active_days.len() as u32,
        first_date,
        last_date,
        top_projects,
        project_clusters,
        tool_frequencies,
        plans_count,
        scheduled_tasks_count,
        memory_files_count,
    })
}

#[derive(Default)]
struct ProjectAccumulator {
    message_count: u32,
    tool_call_count: u32,
    sessions: std::collections::HashSet<String>,
    last_activity: String,
}

/// Best-effort decode of project slug to a human-readable name.
/// e.g. "-Users-tea-Dropbox---apr-2026----ACTION-HEALTH" → "ACTION-HEALTH"
fn prettify_slug(slug: &str) -> String {
    // Strip the leading -Users-<name>-... prefix, keep the suffix.
    let trimmed = slug.trim_start_matches('-');
    if let Some(idx) = trimmed.find("ACTION-") {
        let tail = &trimmed[idx..];
        return tail.replace("---", "-").replace("--", "-").to_string();
    }
    // Otherwise return the last meaningful segment.
    trimmed
        .rsplit('-')
        .find(|seg| !seg.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// Cluster projects by the token after "ACTION-" in their slug, e.g.
/// "...-ACTION-HEALTH" and "...-ACTION-HEALTH---pohwer-..." both go to "ACTION-HEALTH".
/// Projects without "ACTION-" go into "Other".
fn build_clusters(projects: &[(String, u32)]) -> Vec<ClusterStat> {
    let mut by_key: HashMap<String, (u32, u32)> = HashMap::new();
    for (slug, msgs) in projects {
        let key = cluster_key(slug);
        let entry = by_key.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += *msgs;
    }
    let mut out: Vec<ClusterStat> = by_key
        .into_iter()
        .map(|(k, (pc, mc))| ClusterStat {
            key: k,
            project_count: pc,
            message_count: mc,
        })
        .collect();
    out.sort_by(|a, b| b.message_count.cmp(&a.message_count));
    out
}

fn cluster_key(slug: &str) -> String {
    if let Some(idx) = slug.find("ACTION-") {
        let after = &slug[idx + "ACTION-".len()..];
        // Take up to the next "-" or end as the category.
        let category: String = after
            .chars()
            .take_while(|c| *c != '-')
            .collect();
        if !category.is_empty() {
            return format!("ACTION-{category}");
        }
    }
    "Other".to_string()
}

fn count_files(dir: std::path::PathBuf) -> u32 {
    fs::read_dir(&dir)
        .map(|it| it.flatten().filter(|e| e.path().is_file()).count() as u32)
        .unwrap_or(0)
}

fn count_dir_entries(dir: std::path::PathBuf) -> u32 {
    fs::read_dir(&dir)
        .map(|it| it.flatten().count() as u32)
        .unwrap_or(0)
}

fn count_memory_files(projects_dir: &std::path::Path) -> u32 {
    let mut total = 0u32;
    if let Ok(entries) = fs::read_dir(projects_dir) {
        for e in entries.flatten() {
            let memory_dir = e.path().join("memory");
            if memory_dir.is_dir() {
                if let Ok(it) = fs::read_dir(&memory_dir) {
                    for f in it.flatten() {
                        if f.path().extension().and_then(|s| s.to_str()) == Some("md") {
                            total += 1;
                        }
                    }
                }
            }
        }
    }
    total
}

fn format_date(secs: i64) -> String {
    // Cheap UTC date in YYYY-MM-DD format (no chrono dep).
    let days = secs / 86400;
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
    format!("{y:04}-{m:02}-{d:02}")
}
