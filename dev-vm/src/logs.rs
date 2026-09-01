use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn project_log_path(log_dir: &Path, project_id: Uuid) -> PathBuf {
    log_dir.join(project_id.to_string()).join("project.log")
}

pub fn ingress_log_candidates(log_dir: &Path, project_id: Uuid) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let id_str = project_id.to_string();

    // 1. Directly in log_dir (e.g. tests or custom configuration)
    candidates.push(log_dir.join(&id_str).join("ingress.log"));
    candidates.push(log_dir.join(format!("{}.ingress.log", id_str)));

    // 2. Relative to DEVVM_ROOT env var
    if let Ok(devvm_root) = std::env::var("DEVVM_ROOT") {
        let p = PathBuf::from(devvm_root);
        candidates.push(p.join(".ingress-logs").join(&id_str).join("ingress.log"));
        candidates.push(p.join("logs").join(&id_str).join("ingress.log"));
    }

    // 3. Relative to DEVVM_HOME env var
    if let Ok(devvm_home) = std::env::var("DEVVM_HOME") {
        let p = PathBuf::from(devvm_home);
        candidates.push(
            p.join("root/.ingress-logs")
                .join(&id_str)
                .join("ingress.log"),
        );
        candidates.push(p.join("root/logs").join(&id_str).join("ingress.log"));
    }

    // 4. Relative to current working directory
    candidates.push(
        PathBuf::from("root/.ingress-logs")
            .join(&id_str)
            .join("ingress.log"),
    );
    candidates.push(PathBuf::from("root/logs").join(&id_str).join("ingress.log"));
    candidates.push(
        PathBuf::from(".ingress-logs")
            .join(&id_str)
            .join("ingress.log"),
    );

    // 5. Relative to home directory
    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("coding-config/dev-vm/root/.ingress-logs")
                .join(&id_str)
                .join("ingress.log"),
        );
        candidates.push(home.join(".ingress-logs").join(&id_str).join("ingress.log"));
    }

    // 6. Relative to log_dir's parent
    if let Some(parent) = log_dir.parent() {
        candidates.push(
            parent
                .join("root/.ingress-logs")
                .join(&id_str)
                .join("ingress.log"),
        );
        candidates.push(
            parent
                .join(".ingress-logs")
                .join(&id_str)
                .join("ingress.log"),
        );
    }

    candidates
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

    Some(String::from_utf8_lossy(&buffer).into_owned())
}

pub fn append_log(log_dir: &Path, project_id: Uuid, source: &str, message: &str) -> io::Result<()> {
    let path = project_log_path(log_dir, project_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => {
            let secs = d.as_secs();
            format!("{}", secs)
        }
        Err(_) => "0".to_string(),
    };

    let mut entry = String::new();
    for line in message.lines() {
        writeln!(entry, "[{}] [{}] {}", timestamp, source, line)
            .expect("writing to a String cannot fail");
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(entry.as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn read_recent_logs(log_dir: &Path, project_id: Uuid, max_bytes: usize) -> String {
    let project_log_str = read_file_tail(&project_log_path(log_dir, project_id), max_bytes);

    let ingress_log_str = ingress_log_candidates(log_dir, project_id)
        .into_iter()
        .filter_map(|path| {
            let modified = path.metadata().ok()?.modified().ok()?;
            let content = read_file_tail(&path, max_bytes)?;
            (!content.trim().is_empty()).then_some((modified, content))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, content)| content);

    match (project_log_str, ingress_log_str) {
        (Some(mut p_logs), Some(i_logs)) => {
            if !p_logs.is_empty() && !p_logs.ends_with('\n') {
                p_logs.push('\n');
            }
            for line in i_logs.lines() {
                let _ = writeln!(p_logs, "[ingress] {}", line);
            }
            p_logs
        }
        (Some(p_logs), None) => p_logs,
        (None, Some(i_logs)) => {
            let mut formatted = String::new();
            for line in i_logs.lines() {
                let _ = writeln!(formatted, "[ingress] {}", line);
            }
            formatted
        }
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_append_and_read() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        append_log(&log_dir, project_id, "daemon", "Starting VM").unwrap();
        append_log(&log_dir, project_id, "dsh", "DSH listening on port 3080").unwrap();

        let logs = read_recent_logs(&log_dir, project_id, 65536);
        assert!(logs.contains("[daemon] Starting VM"));
        assert!(logs.contains("[dsh] DSH listening on port 3080"));
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

        let logs = read_recent_logs(&log_dir, project_id, 120);
        assert!(logs.starts_with('['), "log tail began mid-entry: {logs:?}");
        assert!(
            !logs.starts_with('�'),
            "log tail began mid-character: {logs:?}"
        );
    }

    #[test]
    fn test_log_read_merges_ingress_log() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        append_log(&log_dir, project_id, "daemon", "Starting VM").unwrap();

        // Create ingress.log
        let ingress_dir = log_dir.join(project_id.to_string());
        fs::create_dir_all(&ingress_dir).unwrap();
        fs::write(
            ingress_dir.join("ingress.log"),
            "2026/04/10 12:00:00 [INFO] [client] [10080.my-proj] start proxy success\n",
        )
        .unwrap();

        let logs = read_recent_logs(&log_dir, project_id, 65536);
        assert!(logs.contains("[daemon] Starting VM"));
        assert!(
            logs.lines().any(|line| line.starts_with("[ingress] ")),
            "ingress lines must keep their own timestamp instead of receiving a read-time timestamp: {logs}"
        );
        assert!(logs.contains("start proxy success"));
    }

    #[test]
    fn test_log_read_ingress_only_when_no_project_log() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let project_id = Uuid::new_v4();

        // Create ingress.log only
        let ingress_dir = log_dir.join(project_id.to_string());
        fs::create_dir_all(&ingress_dir).unwrap();
        fs::write(
            ingress_dir.join("ingress.log"),
            "[INFO] [Caddy] serving on :10080\n[INFO] [client] start proxy success\n",
        )
        .unwrap();

        let logs = read_recent_logs(&log_dir, project_id, 65536);
        assert!(logs.contains("[ingress]"));
        assert!(logs.contains("serving on :10080"));
        assert!(logs.contains("start proxy success"));
    }
}
