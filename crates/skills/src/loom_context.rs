use std::path::Path;

pub struct LoomContext;

impl LoomContext {
    /// Load loom.md from global data_dir and project cwd, concatenated.
    /// Global loom.md comes first, then project loom.md.
    /// Returns empty string if neither file exists.
    pub fn load(data_dir: &Path, cwd: &Path) -> String {
        let global_path = data_dir.join("loom.md");
        let project_path = cwd.join("loom.md");

        let global = std::fs::read_to_string(&global_path).unwrap_or_default();
        let project = std::fs::read_to_string(&project_path).unwrap_or_default();

        match (global.is_empty(), project.is_empty()) {
            (true, true) => String::new(),
            (false, true) => global,
            (true, false) => project,
            (false, false) => format!("{}\n\n{}", global, project),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_no_loom_md_returns_empty() {
        let data_dir = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        let result = LoomContext::load(data_dir.path(), cwd.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_project_loom_md_only() {
        let data_dir = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        std::fs::write(cwd.path().join("loom.md"), "project instructions").unwrap();
        let result = LoomContext::load(data_dir.path(), cwd.path());
        assert_eq!(result, "project instructions");
    }

    #[test]
    fn test_global_loom_md_only() {
        let data_dir = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        std::fs::write(data_dir.path().join("loom.md"), "global instructions").unwrap();
        let result = LoomContext::load(data_dir.path(), cwd.path());
        assert_eq!(result, "global instructions");
    }

    #[test]
    fn test_both_loom_md_merged() {
        let data_dir = tempdir().unwrap();
        let cwd = tempdir().unwrap();
        std::fs::write(data_dir.path().join("loom.md"), "global first").unwrap();
        std::fs::write(cwd.path().join("loom.md"), "project second").unwrap();
        let result = LoomContext::load(data_dir.path(), cwd.path());
        assert_eq!(result, "global first\n\nproject second");
    }
}
