use crate::models::{BrowserEntry, BrowserResult};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum BrowserError {
    AccessDenied(String),
    PathNotFound(String),
    NotADirectory(String),
    IoError(io::Error),
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrowserError::AccessDenied(msg) => write!(f, "Access denied: {}", msg),
            BrowserError::PathNotFound(msg) => write!(f, "Path not found: {}", msg),
            BrowserError::NotADirectory(msg) => write!(f, "Not a directory: {}", msg),
            BrowserError::IoError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for BrowserError {}

impl From<io::Error> for BrowserError {
    fn from(e: io::Error) -> Self {
        BrowserError::IoError(e)
    }
}

pub fn browse_directory(
    home_dir: &Path,
    requested_path: Option<&str>,
) -> Result<BrowserResult, BrowserError> {
    let canonical_home = fs::canonicalize(home_dir).map_err(|e| {
        BrowserError::AccessDenied(format!("Failed to resolve home directory: {}", e))
    })?;

    let target_path = match requested_path {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => canonical_home.clone(),
    };

    if !target_path.exists() {
        return Err(BrowserError::PathNotFound(
            target_path.display().to_string(),
        ));
    }

    let canonical_target = fs::canonicalize(&target_path)
        .map_err(|e| BrowserError::PathNotFound(format!("Cannot canonicalize path: {}", e)))?;

    if !canonical_target.is_dir() {
        return Err(BrowserError::NotADirectory(
            canonical_target.display().to_string(),
        ));
    }

    // Home Jail Boundary Check
    if !canonical_target.starts_with(&canonical_home) {
        return Err(BrowserError::AccessDenied(
            "Browsing outside daemon user's home directory is forbidden".to_string(),
        ));
    }

    let parent = if canonical_target != canonical_home {
        canonical_target.parent().and_then(|p| {
            if p.starts_with(&canonical_home) {
                Some(p.display().to_string())
            } else {
                None
            }
        })
    } else {
        None
    };

    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&canonical_target) {
        for entry in read_dir.flatten() {
            let entry_path = entry.path();
            let file_type = entry.file_type();
            let is_dir = file_type.map(|ft| ft.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip . and ..
            if name == "." || name == ".." {
                continue;
            }

            if is_dir {
                entries.push(BrowserEntry {
                    name,
                    path: entry_path.display().to_string(),
                    is_dir: true,
                });
            }
        }
    }

    entries.sort_by_key(|a| a.name.to_lowercase());

    Ok(BrowserResult {
        current: canonical_target.display().to_string(),
        parent,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_browser_home_jail() {
        let root = tempdir().unwrap();
        let home = root.path().join("home_user");
        let outside = root.path().join("outside");
        let sub = home.join("projects").join("my_proj");

        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(&outside).unwrap();

        // Browsing inside home
        let result = browse_directory(&home, None).unwrap();
        assert_eq!(
            result.current,
            fs::canonicalize(&home).unwrap().display().to_string()
        );
        assert_eq!(result.parent, None);
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].name, "projects");

        // Browsing subfolder
        let result_sub = browse_directory(&home, Some(sub.to_str().unwrap())).unwrap();
        assert_eq!(
            result_sub.current,
            fs::canonicalize(&sub).unwrap().display().to_string()
        );
        assert!(result_sub.parent.is_some());

        // Browsing outside home must fail
        let err = browse_directory(&home, Some(outside.to_str().unwrap())).unwrap_err();
        match err {
            BrowserError::AccessDenied(_) => {}
            _ => panic!("Expected AccessDenied error, got: {:?}", err),
        }

        // Browsing parent traversal out of home must fail
        let traversal = format!("{}/../../", home.to_str().unwrap());
        let err2 = browse_directory(&home, Some(&traversal)).unwrap_err();
        match err2 {
            BrowserError::AccessDenied(_) => {}
            _ => panic!("Expected AccessDenied error, got: {:?}", err2),
        }
    }
}
