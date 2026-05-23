use loom_absolute_path::AbsolutePathBuf;

/// Returns the path to the openLoom data directory, which can be
/// specified by the `LOOM_HOME` environment variable. If not set, defaults to
/// the platform-appropriate data directory:
///
/// - Windows: `%APPDATA%/openLoom/`
/// - macOS:   `~/Library/Application Support/openLoom/`
/// - Linux:   `~/.local/share/openLoom/`
///
/// - If `LOOM_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `LOOM_HOME` is not set, this function does not verify that the
///   directory exists.
pub fn find_loom_home() -> std::io::Result<AbsolutePathBuf> {
    let loom_home_env = std::env::var("LOOM_HOME")
        .ok()
        .filter(|val| !val.is_empty());
    find_loom_home_from_env(loom_home_env.as_deref())
}

/// Alias for backward compatibility with codex-ported code.
#[inline]
pub fn find_codex_home() -> std::io::Result<AbsolutePathBuf> {
    find_loom_home()
}

fn find_loom_home_from_env(loom_home_env: Option<&str>) -> std::io::Result<AbsolutePathBuf> {
    match loom_home_env {
        Some(val) => {
            let path = std::path::PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("LOOM_HOME points to {val:?}, but that path does not exist"),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read LOOM_HOME {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("LOOM_HOME points to {val:?}, but that path is not a directory"),
                ))
            } else {
                let canonical = path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize LOOM_HOME {val:?}: {err}"),
                    )
                })?;
                AbsolutePathBuf::from_absolute_path(canonical)
            }
        }
        None => {
            let mut p = dirs::data_dir().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find data directory",
                )
            })?;
            p.push("openLoom");
            AbsolutePathBuf::from_absolute_path(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find_loom_home_from_env;
    use loom_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn loom_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-loom-home");
        let missing_str = missing
            .to_str()
            .expect("missing loom home path should be valid utf-8");

        let err = find_loom_home_from_env(Some(missing_str)).expect_err("missing LOOM_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("LOOM_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn loom_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("loom-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file loom home path should be valid utf-8");

        let err = find_loom_home_from_env(Some(file_str)).expect_err("file LOOM_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn loom_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp loom home path should be valid utf-8");

        let resolved = find_loom_home_from_env(Some(temp_str)).expect("valid LOOM_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn loom_home_without_env_uses_default_data_dir() {
        let resolved =
            find_loom_home_from_env(None).expect("default LOOM_HOME");
        let mut expected = dirs::data_dir().expect("data dir");
        expected.push("openLoom");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }
}
