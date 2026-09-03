use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// One parsed line of a Project log, merged across the three log files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO-8601 UTC, or empty for an unparseable line at the head of a file.
    pub ts: String,
    /// `daemon` | `dsh` | `ingress`
    pub source: String,
    /// `info` | `warn` | `error`
    pub level: String,
    pub message: String,
}

/// The Project log directory, shared with the DevVM: the daemon writes `daemon.log`, the
/// guest writes `dsh.log` and `ingress.log` into the same directory.
pub fn project_log_dir(log_dir: &Path, project_id: Uuid) -> PathBuf {
    log_dir.join(project_id.to_string())
}

pub fn daemon_log_path(log_dir: &Path, project_id: Uuid) -> PathBuf {
    project_log_dir(log_dir, project_id).join("daemon.log")
}

/// Formats epoch milliseconds as `YYYY-MM-DDTHH:MM:SS.mmmZ` (ISO 8601, UTC, milliseconds).
fn format_iso8601_millis(epoch_millis: u64) -> String {
    let seconds = epoch_millis / 1000;
    let time_of_day = seconds % 86_400;
    // civil_from_days (Howard Hinnant): the era starts in March so leap days land last.
    let shifted = (seconds / 86_400) as i64 + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year,
        month,
        day,
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
        epoch_millis % 1000
    )
}

fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn read_file_tail(path: &Path, max_bytes: usize) -> Option<String> {
    if !path.exists() {
        return None;
    }

    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len() as usize;
    let start = file_len.saturating_sub(max_bytes);

    if start > 0 {
        file.seek(SeekFrom::Start((start - 1) as u64)).ok()?;
    }

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;

    if start > 0 {
        if buffer.first() == Some(&b'\n') {
            buffer.remove(0);
        } else if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            buffer.drain(..=newline);
        } else {
            buffer.clear();
        }
    }

    if buffer.last() != Some(&b'\n') {
        if let Some(newline) = buffer.iter().rposition(|byte| *byte == b'\n') {
            buffer.truncate(newline + 1);
        } else {
            buffer.clear();
        }
    }

    Some(String::from_utf8_lossy(&buffer).into_owned())
}

fn strip_terminal_controls(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            0x1b => {
                index += 1;
                match bytes.get(index) {
                    Some(b'[') => {
                        index += 1;
                        while index < bytes.len() {
                            let byte = bytes[index];
                            index += 1;
                            if (0x40..=0x7e).contains(&byte) {
                                break;
                            }
                        }
                    }
                    Some(b']') => {
                        index += 1;
                        while index < bytes.len() {
                            if bytes[index] == 0x07 {
                                index += 1;
                                break;
                            }
                            if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
                                index += 2;
                                break;
                            }
                            index += 1;
                        }
                    }
                    Some(_) => {
                        while index < bytes.len() {
                            let byte = bytes[index];
                            index += 1;
                            if (0x30..=0x7e).contains(&byte) {
                                break;
                            }
                        }
                    }
                    None => {}
                }
            }
            b'\t' | b'\n' | 0x20..=0x7e | 0x80..=0xff => {
                output.push(bytes[index]);
                index += 1;
            }
            _ => index += 1,
        }
    }

    String::from_utf8_lossy(&output).into_owned()
}

pub fn append_log(log_dir: &Path, project_id: Uuid, source: &str, message: &str) -> io::Result<()> {
    let path = daemon_log_path(log_dir, project_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let timestamp = format_iso8601_millis(now_epoch_millis());

    let mut entry = String::new();
    for line in message.lines() {
        let clean = strip_terminal_controls(line);
        if !clean.trim().is_empty() {
            writeln!(entry, "[{}] [{}] {}", timestamp, source, clean)
                .expect("writing to a String cannot fail");
        }
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(entry.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Appends to a Project's log, logging a failure to write instead of dropping it.
pub fn append_log_logged(log_dir: &Path, project_id: Uuid, source: &str, message: &str) {
    if let Err(e) = append_log(log_dir, project_id, source, message) {
        tracing::error!(
            project = ?project_id,
            source,
            error = %e,
            "failed to append to project log"
        );
    }
}

fn level_for_tag(suffix: &str) -> &'static str {
    match suffix {
        "error" | "err" => "error",
        "warn" | "warning" => "warn",
        _ => "info",
    }
}

fn looks_like_iso_ts(candidate: &str) -> bool {
    candidate.len() == 24
        && candidate.ends_with('Z')
        && candidate[..4].starts_with(|c: char| c.is_ascii_digit())
}

/// Parses a `daemon.log` line: `[ISO] [tag] text`. A tag that is not a known source keeps its
/// name as a message prefix (`devvm: ...`) and reports the entry as `daemon`.
pub fn parse_daemon_line(line: &str) -> Option<LogEntry> {
    let (ts, rest) = line.strip_prefix('[')?.split_once("] ")?;
    if !looks_like_iso_ts(ts) {
        return None;
    }
    let (tag, text) = rest.strip_prefix('[')?.split_once("] ")?;

    let (name, level) = match tag.split_once(':') {
        Some((name, suffix)) => (name, level_for_tag(suffix)),
        None => (tag, "info"),
    };

    let (source, message) = if matches!(name, "daemon" | "dsh" | "ingress") {
        (name.to_string(), text.to_string())
    } else {
        ("daemon".to_string(), format!("{}: {}", name, text))
    };

    Some(LogEntry {
        ts: ts.to_string(),
        source,
        level: level.to_string(),
        message,
    })
}

/// Parses a guest-written line: `[ISO] text` (`dsh.log`, frpc lines in `ingress.log`).
pub fn parse_prefixed_line(source: &str, line: &str) -> Option<LogEntry> {
    let (ts, text) = line.strip_prefix('[')?.split_once("] ")?;
    if !looks_like_iso_ts(ts) {
        return None;
    }

    let level =
        if text.starts_with("Error") || text.starts_with("error:") || text.contains(" ERROR ") {
            "error"
        } else {
            "info"
        };

    Some(LogEntry {
        ts: ts.to_string(),
        source: source.to_string(),
        level: level.to_string(),
        message: text.to_string(),
    })
}

/// Parses a Caddy JSON line, compacting request logs to `METHOD URI → STATUS (D.d ms)`.
pub fn parse_caddy_line(line: &str) -> Option<LogEntry> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let object = value.as_object()?;
    let seconds = object.get("ts")?.as_f64()?;

    let level = match object.get("level").and_then(|v| v.as_str()) {
        Some("error") => "error",
        Some("warn") => "warn",
        _ => "info",
    };
    let msg = object
        .get("msg")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let mut message = match object.get("request").and_then(|v| v.as_object()) {
        Some(request) => {
            let method = request
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let uri = request.get("uri").and_then(|v| v.as_str()).unwrap_or("?");
            let mut compact = format!("{} {}", method, uri);
            // `http.log.error` entries often carry no status; then the arrow part is dropped.
            if let Some(status) = object.get("status").and_then(|v| v.as_u64()) {
                let duration_ms = object
                    .get("duration")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    * 1000.0;
                let _ = write!(compact, " → {} ({:.1} ms)", status, duration_ms);
            }
            if !msg.is_empty() && msg != "handled request" {
                let _ = write!(compact, " — {}", msg);
            }
            compact
        }
        None => msg.to_string(),
    };
    if message.is_empty() {
        message = line.trim().to_string();
    }

    Some(LogEntry {
        ts: format_iso8601_millis((seconds * 1000.0).round() as u64),
        source: "ingress".to_string(),
        level: level.to_string(),
        message,
    })
}

/// Parses one log file's text, appending its entries. An unparseable line keeps the raw text
/// with the previous entry's timestamp, which is empty for lines at the head of the file.
pub fn parse_file(source: &str, text: &str, entries: &mut Vec<LogEntry>) {
    let mut last_ts = String::new();
    for line in text.lines() {
        let clean = strip_terminal_controls(line);
        let clean = clean.trim_end();
        if clean.trim().is_empty() {
            continue;
        }

        let parsed = if clean.starts_with('{') {
            parse_caddy_line(clean)
        } else if source == "daemon" {
            parse_daemon_line(clean)
        } else {
            parse_prefixed_line(source, clean)
        };

        let entry = parsed.unwrap_or_else(|| LogEntry {
            ts: last_ts.clone(),
            source: source.to_string(),
            level: "info".to_string(),
            message: clean.to_string(),
        });
        last_ts = entry.ts.clone();
        entries.push(entry);
    }
}

/// Reads the tail of the Project's three log files and merges them into one time-ordered list.
/// Entries whose timestamp is empty (unparseable head-of-file lines) sort first.
pub fn read_recent_logs(log_dir: &Path, project_id: Uuid, max_bytes: usize) -> Vec<LogEntry> {
    let dir = project_log_dir(log_dir, project_id);
    let mut entries = Vec::new();
    for source in ["daemon", "dsh", "ingress"] {
        if let Some(content) = read_file_tail(&dir.join(format!("{}.log", source)), max_bytes) {
            parse_file(source, &content, &mut entries);
        }
    }
    entries.sort_by(|a, b| a.ts.cmp(&b.ts));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_format_iso8601_millis_matches_known_epochs() {
        assert_eq!(format_iso8601_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_iso8601_millis(1_772_500_516_210),
            "2026-03-03T01:15:16.210Z"
        );
        // 2024-02-29 is a leap day; 2100-03-01 follows a skipped leap year.
        assert_eq!(
            format_iso8601_millis(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
        assert_eq!(
            format_iso8601_millis(4_107_542_400_000),
            "2100-03-01T00:00:00.000Z"
        );
    }

    #[test]
    fn test_append_log_writes_iso_timestamped_daemon_log() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        append_log(&log_dir, project_id, "daemon", "Starting VM").unwrap();

        let content = fs::read_to_string(daemon_log_path(&log_dir, project_id)).unwrap();
        let line = content.lines().next().unwrap();
        let (timestamp, rest) = line.split_once("] ").unwrap();
        let timestamp = timestamp.strip_prefix('[').unwrap();
        assert_eq!(timestamp.len(), 24, "timestamp was {timestamp}");
        assert_eq!(&timestamp[10..11], "T");
        assert!(timestamp.ends_with('Z'));
        assert_eq!(rest, "[daemon] Starting VM");
    }

    #[test]
    fn test_log_append_and_read() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        append_log(&log_dir, project_id, "daemon", "Starting VM").unwrap();
        append_log(
            &log_dir,
            project_id,
            "dsh",
            "\u{1b}[1;34mDSH listening on port 3080\u{1b}[0m\n\u{1b}[0m",
        )
        .unwrap();

        let entries = read_recent_logs(&log_dir, project_id, 65536);
        assert!(entries
            .iter()
            .any(|e| e.source == "daemon" && e.message == "Starting VM"));
        assert!(entries
            .iter()
            .any(|e| e.source == "dsh" && e.message == "DSH listening on port 3080"));
        assert!(entries.iter().all(|e| !e.message.contains('\u{1b}')));
        assert!(entries.iter().all(|e| !e.message.trim().is_empty()));
    }

    #[test]
    fn test_log_tail_starts_at_a_complete_entry() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        for index in 0..20 {
            append_log(
                &log_dir,
                project_id,
                "test",
                &format!("entry {index}: Hello 🦀 Rust"),
            )
            .unwrap();
        }

        let entries = read_recent_logs(&log_dir, project_id, 120);
        let first = entries.first().expect("expected a tail entry");
        assert!(!first.ts.is_empty(), "log tail began mid-entry: {first:?}");
        assert!(
            first.message.starts_with("test: entry"),
            "log tail began mid-entry: {first:?}"
        );
    }

    #[test]
    fn test_log_read_merges_guest_written_logs() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        append_log(&log_dir, project_id, "daemon", "Starting VM").unwrap();

        let guest_dir = project_log_dir(&log_dir, project_id);
        fs::create_dir_all(&guest_dir).unwrap();
        fs::write(
            guest_dir.join("dsh.log"),
            "[2026-09-02T22:15:16.210Z] dsh web: http://127.0.0.1:3080\n",
        )
        .unwrap();
        fs::write(
            guest_dir.join("ingress.log"),
            "[2026-09-02T22:15:17.001Z] [client] [10080.my-proj] start proxy success\n",
        )
        .unwrap();

        let entries = read_recent_logs(&log_dir, project_id, 65536);
        assert!(entries
            .iter()
            .any(|e| e.source == "daemon" && e.message == "Starting VM"));
        assert!(entries.iter().any(|e| e.source == "dsh"
            && e.ts == "2026-09-02T22:15:16.210Z"
            && e.message == "dsh web: http://127.0.0.1:3080"));
        assert!(
            entries.iter().any(|e| e.source == "ingress"
                && e.ts == "2026-09-02T22:15:17.001Z"
                && e.message.contains("start proxy success")),
            "guest lines must keep their own timestamp instead of receiving a read-time timestamp: {entries:?}"
        );
    }

    #[test]
    fn test_log_read_ingress_only_when_no_daemon_log() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        let guest_dir = project_log_dir(&log_dir, project_id);
        fs::create_dir_all(&guest_dir).unwrap();
        fs::write(
            guest_dir.join("ingress.log"),
            "[INFO] [Caddy] serving on :10080\n[INFO] [client] start proxy success\n",
        )
        .unwrap();

        let entries = read_recent_logs(&log_dir, project_id, 65536);
        assert!(entries.iter().all(|e| e.source == "ingress"));
        assert!(entries
            .iter()
            .any(|e| e.message.contains("serving on :10080")));
        assert!(entries
            .iter()
            .any(|e| e.message.contains("start proxy success")));
    }

    #[test]
    fn test_parse_daemon_line_formats() {
        assert_eq!(
            parse_daemon_line("[2026-09-02T22:15:16.210Z] [daemon] Starting VM"),
            Some(LogEntry {
                ts: "2026-09-02T22:15:16.210Z".to_string(),
                source: "daemon".to_string(),
                level: "info".to_string(),
                message: "Starting VM".to_string(),
            })
        );
        assert_eq!(
            parse_daemon_line("[2026-09-02T22:15:16.210Z] [daemon:error] devvm start failed"),
            Some(LogEntry {
                ts: "2026-09-02T22:15:16.210Z".to_string(),
                source: "daemon".to_string(),
                level: "error".to_string(),
                message: "devvm start failed".to_string(),
            })
        );
        assert_eq!(
            parse_daemon_line("[2026-09-02T22:15:16.210Z] [sync:warn] remote ahead"),
            Some(LogEntry {
                ts: "2026-09-02T22:15:16.210Z".to_string(),
                source: "daemon".to_string(),
                level: "warn".to_string(),
                message: "sync: remote ahead".to_string(),
            })
        );
        assert_eq!(parse_daemon_line("not a log line"), None);
    }

    #[test]
    fn test_parse_prefixed_line_levels() {
        assert_eq!(
            parse_prefixed_line("dsh", "[2026-09-02T22:15:16.210Z] dsh web: http://x"),
            Some(LogEntry {
                ts: "2026-09-02T22:15:16.210Z".to_string(),
                source: "dsh".to_string(),
                level: "info".to_string(),
                message: "dsh web: http://x".to_string(),
            })
        );
        assert_eq!(
            parse_prefixed_line("dsh", "[2026-09-02T22:15:16.210Z] Error: plugin missing")
                .unwrap()
                .level,
            "error"
        );
        assert_eq!(
            parse_prefixed_line("ingress", "[2026-09-02T22:15:16.210Z] x ERROR y")
                .unwrap()
                .level,
            "error"
        );
    }

    #[test]
    fn test_parse_caddy_line_compacts_requests() {
        let entry = parse_caddy_line(
            r#"{"level":"error","ts":1772500516.21,"logger":"http.log.access","msg":"handled request","request":{"method":"GET","uri":"/api/events.mux"},"status":502,"duration":0.000203}"#,
        )
        .unwrap();
        assert_eq!(entry.ts, "2026-03-03T01:15:16.210Z");
        assert_eq!(entry.source, "ingress");
        assert_eq!(entry.level, "error");
        assert_eq!(entry.message, "GET /api/events.mux → 502 (0.2 ms)");

        let no_status = parse_caddy_line(
            r#"{"level":"error","ts":1772500516.21,"logger":"http.log.error","msg":"dial tcp: connection refused","request":{"method":"GET","uri":"/"}}"#,
        )
        .unwrap();
        assert_eq!(no_status.message, "GET / — dial tcp: connection refused");

        let plain =
            parse_caddy_line(r#"{"level":"info","ts":1772500516.21,"msg":"serving"}"#).unwrap();
        assert_eq!(plain.message, "serving");
        assert_eq!(plain.level, "info");
    }

    #[test]
    fn test_parse_file_keeps_unparseable_line_with_previous_timestamp() {
        let mut entries = Vec::new();
        parse_file(
            "dsh",
            "raw head line\n[2026-09-02T22:15:16.210Z] started\nstack trace line\n",
            &mut entries,
        );

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].ts, "");
        assert_eq!(entries[0].message, "raw head line");
        assert_eq!(entries[2].ts, "2026-09-02T22:15:16.210Z");
        assert_eq!(entries[2].message, "stack trace line");
    }

    #[test]
    fn test_read_recent_logs_merges_files_in_time_order() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();
        let guest_dir = project_log_dir(&log_dir, project_id);
        fs::create_dir_all(&guest_dir).unwrap();

        fs::write(
            guest_dir.join("daemon.log"),
            "[2026-09-02T22:15:03.000Z] [daemon] third\n",
        )
        .unwrap();
        fs::write(
            guest_dir.join("dsh.log"),
            "[2026-09-02T22:15:01.000Z] first\n",
        )
        .unwrap();
        fs::write(
            guest_dir.join("ingress.log"),
            "[2026-09-02T22:15:02.000Z] second\n",
        )
        .unwrap();

        let entries = read_recent_logs(&log_dir, project_id, 65536);
        let order: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(order, vec!["first", "second", "third"]);
        let sources: Vec<&str> = entries.iter().map(|e| e.source.as_str()).collect();
        assert_eq!(sources, vec!["dsh", "ingress", "daemon"]);
    }
}
