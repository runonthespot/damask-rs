use std::fs;
use std::path::{Path, PathBuf};

use damask_core::DamaskConfig;

use crate::StoreError;

/// Represents a Damask project rooted at a `.damask/` directory.
pub struct DamaskProject {
    /// Path to the `.damask/` directory itself.
    pub damask_dir: PathBuf,
    /// Path to the project root (parent of `.damask/`).
    pub root: PathBuf,
}

impl DamaskProject {
    /// Initialize a new `.damask/` directory in the given project root.
    /// Creates the directory structure: `.damask/edges/`, `.damask/config.json`.
    pub fn init(root: &Path) -> Result<Self, StoreError> {
        let damask_dir = root.join(".damask");
        if damask_dir.exists() {
            return Err(StoreError::AlreadyInitialized(
                damask_dir.display().to_string(),
            ));
        }

        fs::create_dir_all(damask_dir.join("edges")).map_err(|e| StoreError::Io(e.to_string()))?;

        let config = DamaskConfig::default();
        let config_json = serde_json::to_string_pretty(&config).map_err(StoreError::Json)?;
        fs::write(damask_dir.join("config.json"), config_json)
            .map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(Self {
            root: root.to_path_buf(),
            damask_dir,
        })
    }

    /// Discover an existing `.damask/` directory by searching upward from `start`.
    pub fn discover(start: &Path) -> Result<Self, StoreError> {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join(".damask");
            if candidate.is_dir() {
                return Ok(Self {
                    root: current,
                    damask_dir: candidate,
                });
            }
            if !current.pop() {
                return Err(StoreError::NotFound);
            }
        }
    }

    /// Path to the JSONL file for a given namespace.
    pub fn edges_file(&self, ns: &str) -> PathBuf {
        self.damask_dir.join("edges").join(format!("{}.jsonl", ns))
    }

    /// Path to the config file.
    pub fn config_path(&self) -> PathBuf {
        self.damask_dir.join("config.json")
    }

    /// Read the project configuration.
    pub fn read_config(&self) -> Result<DamaskConfig, StoreError> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(DamaskConfig::default());
        }
        let content = fs::read_to_string(&path).map_err(|e| StoreError::Io(e.to_string()))?;
        serde_json::from_str(&content).map_err(StoreError::Json)
    }

    /// Path to the active namespace file.
    pub fn active_ns_path(&self) -> PathBuf {
        self.damask_dir.join(".active_ns")
    }

    /// Get the active namespace: `DAMASK_NS` env var > `.damask/.active_ns` file > config `default_ns`.
    pub fn active_ns(&self) -> Option<String> {
        if let Ok(ns) = std::env::var("DAMASK_NS") {
            if !ns.is_empty() {
                return Some(ns);
            }
        }
        let path = self.active_ns_path();
        if let Some(ns) = fs::read_to_string(path).ok().map(|s| s.trim().to_string()) {
            if !ns.is_empty() {
                return Some(ns);
            }
        }
        // Fall back to config default_ns
        self.read_config().ok().and_then(|c| c.default_ns.clone())
    }

    /// Set the active namespace by writing `.damask/.active_ns`.
    pub fn set_active_ns(&self, ns: &str) -> Result<(), StoreError> {
        fs::write(self.active_ns_path(), ns).map_err(|e| StoreError::Io(e.to_string()))
    }

    /// List all namespaces (based on JSONL files in edges/).
    pub fn list_namespaces(&self) -> Result<Vec<String>, StoreError> {
        let edges_dir = self.damask_dir.join("edges");
        if !edges_dir.exists() {
            return Ok(Vec::new());
        }

        let mut namespaces = Vec::new();
        let entries = fs::read_dir(&edges_dir).map_err(|e| StoreError::Io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| StoreError::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Some(stem) = path.file_stem() {
                    namespaces.push(stem.to_string_lossy().to_string());
                }
            }
        }

        namespaces.sort();
        Ok(namespaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_structure() {
        let dir = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(dir.path()).unwrap();

        assert!(project.damask_dir.exists());
        assert!(project.damask_dir.join("edges").is_dir());
        assert!(project.damask_dir.join("config.json").is_file());

        let config = project.read_config().unwrap();
        assert!(config.default_ns.is_none());
    }

    #[test]
    fn init_rejects_existing() {
        let dir = tempfile::tempdir().unwrap();
        DamaskProject::init(dir.path()).unwrap();
        let result = DamaskProject::init(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn discover_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        DamaskProject::init(dir.path()).unwrap();

        let subdir = dir.path().join("src").join("deep");
        fs::create_dir_all(&subdir).unwrap();

        let found = DamaskProject::discover(&subdir).unwrap();
        assert_eq!(found.root, dir.path());
    }

    #[test]
    fn discover_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = DamaskProject::discover(dir.path());
        assert!(matches!(result, Err(StoreError::NotFound)));
    }

    #[test]
    fn edges_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(dir.path()).unwrap();
        let path = project.edges_file("security-audit");
        assert!(path.ends_with(".damask/edges/security-audit.jsonl"));
    }

    #[test]
    fn active_ns_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(dir.path()).unwrap();

        assert!(project.active_ns().is_none());

        project.set_active_ns("my-ns").unwrap();
        // Clear env var in case it's set
        std::env::remove_var("DAMASK_NS");
        assert_eq!(project.active_ns(), Some("my-ns".to_string()));
    }

    #[test]
    fn list_namespaces_from_jsonl_files() {
        let dir = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(dir.path()).unwrap();

        assert!(project.list_namespaces().unwrap().is_empty());

        // Create some JSONL files
        fs::write(project.edges_file("alpha"), "").unwrap();
        fs::write(project.edges_file("beta"), "").unwrap();
        fs::write(project.edges_file("gamma"), "").unwrap();

        let nss = project.list_namespaces().unwrap();
        assert_eq!(nss, vec!["alpha", "beta", "gamma"]);
    }
}
