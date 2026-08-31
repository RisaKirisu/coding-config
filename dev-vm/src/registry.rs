use crate::models::ProjectRecord;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug)]
pub enum RegistryError {
    PathNotFound(String),
    NotADirectory(String),
    IoError(io::Error),
    InvalidIdFile(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::PathNotFound(p) => write!(f, "Path does not exist: {}", p),
            RegistryError::NotADirectory(p) => write!(f, "Path is not a directory: {}", p),
            RegistryError::IoError(e) => write!(f, "I/O error: {}", e),
            RegistryError::InvalidIdFile(s) => write!(f, "Invalid .devvm-id: {}", s),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<io::Error> for RegistryError {
    fn from(e: io::Error) -> Self {
        RegistryError::IoError(e)
    }
}

pub fn load_projects(config_path: &Path) -> Result<Vec<ProjectRecord>, io::Error> {
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(config_path)?;
    if data.trim().is_empty() {
        return Ok(Vec::new());
    }
    let records: Vec<ProjectRecord> = serde_json::from_str(&data).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", config_path.display(), e),
        )
    })?;
    Ok(records)
}

pub fn save_projects(config_path: &Path, projects: &[ProjectRecord]) -> Result<(), io::Error> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json_bytes = serde_json::to_vec_pretty(projects).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Serialization error: {}", e),
        )
    })?;

    // Atomic write via temp file
    let tmp_path = config_path.with_extension(format!("tmp.{}", Uuid::new_v4()));
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(&json_bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, config_path)?;
    Ok(())
}

pub fn ensure_project_id(project_dir: &Path) -> Result<Uuid, RegistryError> {
    let id_path = project_dir.join(".devvm-id");
    if id_path.exists() {
        if let Ok(content) = fs::read_to_string(&id_path) {
            let trimmed = content.trim();
            if let Ok(existing_uuid) = Uuid::parse_str(trimmed) {
                return Ok(existing_uuid);
            }
        }
    }

    // Generate new UUID v4
    let new_uuid = Uuid::new_v4();
    fs::write(&id_path, format!("{}\n", new_uuid))?;
    Ok(new_uuid)
}

pub fn register_project(
    config_path: &Path,
    raw_path: &str,
) -> Result<ProjectRecord, RegistryError> {
    let path = PathBuf::from(raw_path);
    if !path.exists() {
        return Err(RegistryError::PathNotFound(raw_path.to_string()));
    }
    let canonical = fs::canonicalize(&path)?;
    if !canonical.is_dir() {
        return Err(RegistryError::NotADirectory(
            canonical.display().to_string(),
        ));
    }

    let id = ensure_project_id(&canonical)?;

    let mut projects = load_projects(config_path)?;

    // Check if project already registered by path or ID
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .ok();

    if let Some(existing) = projects
        .iter_mut()
        .find(|p| p.path == canonical || p.id == id)
    {
        existing.id = id;
        existing.path = canonical.clone();
        let record = existing.clone();
        save_projects(config_path, &projects)?;
        return Ok(record);
    }

    let new_record = ProjectRecord {
        id,
        path: canonical,
        created_at: now,
    };
    projects.push(new_record.clone());
    save_projects(config_path, &projects)?;

    Ok(new_record)
}

pub fn unregister_project(config_path: &Path, id: Uuid) -> Result<bool, io::Error> {
    let mut projects = load_projects(config_path)?;
    let original_len = projects.len();
    projects.retain(|p| p.id != id);
    if projects.len() != original_len {
        save_projects(config_path, &projects)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn get_project(config_path: &Path, id: Uuid) -> Result<Option<ProjectRecord>, io::Error> {
    let projects = load_projects(config_path)?;
    Ok(projects.into_iter().find(|p| p.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_register_and_unregister() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("my-test-project");
        fs::create_dir_all(&project_dir).unwrap();

        let config_path = dir.path().join("config/projects.json");

        // First registration creates .devvm-id
        let record = register_project(&config_path, project_dir.to_str().unwrap()).unwrap();
        assert!(project_dir.join(".devvm-id").exists());
        let saved_uuid_str = fs::read_to_string(project_dir.join(".devvm-id")).unwrap();
        assert_eq!(record.id.to_string(), saved_uuid_str.trim());

        // Re-registration reuses existing .devvm-id
        let record2 = register_project(&config_path, project_dir.to_str().unwrap()).unwrap();
        assert_eq!(record.id, record2.id);

        // Load projects
        let loaded = load_projects(&config_path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, record.id);

        // Unregister
        let removed = unregister_project(&config_path, record.id).unwrap();
        assert!(removed);
        let loaded2 = load_projects(&config_path).unwrap();
        assert_eq!(loaded2.len(), 0);

        // Verify .devvm-id and project dir remain untouched on unregister
        assert!(project_dir.join(".devvm-id").exists());
        assert!(project_dir.exists());
    }
}
