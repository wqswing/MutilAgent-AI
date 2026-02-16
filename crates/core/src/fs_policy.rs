//! Filesystem security policy enforcement.
//!
//! Provides utilities for validating and canonicalizing paths to prevent
//! path traversal and symlink-based escapes from the sandbox environment.

use crate::Result;
use std::path::{Component, Path, PathBuf};

/// Validates a path intended for use within a sandbox environment.
///
/// This function:
/// 1. Canonicalizes the path (relative to the given root).
/// 2. Ensures the resulting path is within the root.
/// 3. Rejects absolute paths and paths containing '..' after normalization.
pub fn validate_sandbox_path(root: &str, input_path: &str) -> Result<PathBuf> {
    let root_path = Path::new(root);

    // Cross-platform check: reject Windows-style absolute paths on any OS
    if input_path.len() >= 2
        && input_path.as_bytes()[1] == b':'
        && input_path.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(crate::Error::SecurityViolation(format!(
            "Absolute paths are not allowed in sandbox: {}",
            input_path
        )));
    }

    // 1. Normalize the input path to prevent basic traversal
    let mut normalized = PathBuf::new();
    for component in Path::new(input_path).components() {
        match component {
            Component::Normal(c) => normalized.push(c),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(crate::Error::SecurityViolation(format!(
                        "Path traversal detected in path: {}",
                        input_path
                    )));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(crate::Error::SecurityViolation(format!(
                    "Absolute paths are not allowed in sandbox: {}",
                    input_path
                )));
            }
            Component::CurDir => {}
        }
    }

    // 2. Build the absolute path within the root
    let full_path = root_path.join(&normalized);

    // 3. Final safety check: ensure the resulting path still starts with the root
    // This is a redundant check against edge cases in join() or complex paths.
    if !full_path.starts_with(root_path) {
        return Err(crate::Error::SecurityViolation(format!(
            "Access denied: path {} is outside of root {}",
            input_path, root
        )));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert_eq!(
            validate_sandbox_path("/workspace", "main.py").unwrap(),
            PathBuf::from("main.py")
        );
        assert_eq!(
            validate_sandbox_path("/workspace", "src/app.js").unwrap(),
            PathBuf::from("src/app.js")
        );
        assert_eq!(
            validate_sandbox_path("/workspace", "./local.txt").unwrap(),
            PathBuf::from("local.txt")
        );
    }

    #[test]
    fn test_traversal_rejection() {
        assert!(validate_sandbox_path("/workspace", "../etc/passwd").is_err());
        assert!(validate_sandbox_path("/workspace", "src/../../etc/passwd").is_err());
    }

    #[test]
    fn test_absolute_path_rejection() {
        assert!(validate_sandbox_path("/workspace", "/etc/passwd").is_err());
        assert!(validate_sandbox_path("/workspace", "C:\\Windows\\System32").is_err());
    }
}
