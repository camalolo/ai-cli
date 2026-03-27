use difference::{Changeset, Difference};
use regex::Regex;
use std::fs;
use std::path::PathBuf;

use crate::sandbox::get_sandbox_root;
use crate::utils::confirm;
use anyhow::Result;

use crate::patch::apply_patch;

/// Maximum allowed file size for read/write operations (10 MB)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

const CANCELLATION_MESSAGE: &str = "User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.";

pub(crate) fn resolve_sandbox_path(filename: &str) -> Result<PathBuf, String> {
    if std::path::Path::new(filename)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            "Path traversal detected: filename must not contain '..' components".to_string(),
        );
    }

    let joined = PathBuf::from(get_sandbox_root()).join(filename);
    let canonicalized = dunce::canonicalize(&joined)
        .map_err(|e| format!("Failed to resolve path '{}': {}", filename, e))?;

    let sandbox_root = dunce::canonicalize(get_sandbox_root())
        .map_err(|e| format!("Failed to resolve sandbox root: {}", e))?;

    if !canonicalized.starts_with(&sandbox_root) {
        return Err("Access denied: path resolves outside sandbox".to_string());
    }

    Ok(canonicalized)
}

fn confirm_change(
    original: &str,
    new_content: &str,
    filename: &str,
    operation_desc: &str,
) -> Result<bool> {
    let changeset = Changeset::new(original, new_content, "\n");
    println!("Diff preview for {} in '{}':", operation_desc, filename);
    for diff in &changeset.diffs {
        match diff {
            Difference::Same(ref s) => println!("\x1b[2m {}\x1b[0m", s),
            Difference::Rem(ref s) => println!("\x1b[91m-{}\x1b[0m", s),
            Difference::Add(ref s) => println!("\x1b[92m+{}\x1b[0m", s),
        }
    }
    Ok(confirm("Apply changes?"))
}

fn confirm_and_apply_change(
    old_content: &str,
    new_content: &str,
    filename: &str,
    operation_desc: &str,
    skip_confirmation: bool,
) -> Result<(), String> {
    if skip_confirmation {
        return Ok(());
    }
    match confirm_change(old_content, new_content, filename, operation_desc) {
        Ok(true) => Ok(()),
        Ok(false) => Err(CANCELLATION_MESSAGE.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn handle_read(file_path: &PathBuf, filename: &str) -> (String, bool) {
    match fs::metadata(file_path) {
        Ok(metadata) if metadata.len() > MAX_FILE_SIZE => {
            return (
                format!(
                    "Error: File '{}' is too large ({} bytes, max is {} bytes)",
                    filename,
                    metadata.len(),
                    MAX_FILE_SIZE
                ),
                false,
            );
        }
        Ok(_) => {}
        Err(e) => {
            return (
                format!("Error reading file metadata '{}': {}", filename, e),
                false,
            )
        }
    }
    match fs::read_to_string(file_path) {
        Ok(content) => (format!("File contents:\n{}", content), false),
        Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
    }
}

fn handle_write(
    file_path: &PathBuf,
    filename: &str,
    data: Option<&str>,
    skip_confirmation: bool,
) -> (String, bool) {
    let new_content = data.unwrap_or("");
    let content_size = new_content.len() as u64;
    if content_size > MAX_FILE_SIZE {
        return (
            format!(
                "Error: Content too large ({} bytes, max is {} bytes)",
                content_size, MAX_FILE_SIZE
            ),
            false,
        );
    }
    let current_content = fs::read_to_string(file_path).unwrap_or_default();
    if let Err(msg) = confirm_and_apply_change(
        &current_content,
        new_content,
        filename,
        "writing to",
        skip_confirmation,
    ) {
        let is_cancel = msg == CANCELLATION_MESSAGE;
        return (msg, is_cancel);
    }
    match fs::write(file_path, new_content) {
        Ok(()) => (format!("Successfully wrote to '{}'", filename), false),
        Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
    }
}

fn handle_search(file_path: &PathBuf, filename: &str, data: Option<&str>) -> (String, bool) {
    let pattern = match data {
        Some(p) => p,
        None => {
            return (
                "Error: 'data' parameter with regex pattern is required for search".to_string(),
                false,
            );
        }
    };
    match Regex::new(pattern) {
        Ok(re) => {
            match fs::metadata(file_path) {
                Ok(metadata) if metadata.len() > MAX_FILE_SIZE => {
                    return (
                        format!(
                            "Error: File '{}' is too large ({} bytes, max is {} bytes)",
                            filename,
                            metadata.len(),
                            MAX_FILE_SIZE
                        ),
                        false,
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    return (
                        format!("Error reading file metadata '{}': {}", filename, e),
                        false,
                    )
                }
            }
            match fs::read_to_string(file_path) {
                Ok(content) => {
                    let matches: Vec<_> = re.find_iter(&content).collect();
                    if matches.is_empty() {
                        (
                            format!(
                                "No matches found for pattern '{}' in '{}'",
                                pattern, filename
                            ),
                            false,
                        )
                    } else {
                        let match_list: Vec<String> = matches
                            .iter()
                            .map(|m| format!(" - {} (at position {})", m.as_str(), m.start()))
                            .collect();
                        (
                            format!(
                                "Found {} matches for pattern '{}' in '{}':\n{}",
                                matches.len(),
                                pattern,
                                filename,
                                match_list.join("\n")
                            ),
                            false,
                        )
                    }
                }
                Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
            }
        }
        Err(e) => (
            format!("Error compiling regex pattern '{}': {}", pattern, e),
            false,
        ),
    }
}

fn handle_search_and_replace(
    file_path: &PathBuf,
    filename: &str,
    data: Option<&str>,
    replacement: Option<&str>,
    skip_confirmation: bool,
) -> (String, bool) {
    let pattern = match data {
        Some(p) => p,
        None => {
            return (
                "Error: 'data' parameter with regex pattern is required for search_and_replace"
                    .to_string(),
                false,
            )
        }
    };
    let replace_with = match replacement {
        Some(r) => r,
        None => {
            return (
                "Error: 'replacement' parameter is required for search_and_replace".to_string(),
                false,
            );
        }
    };
    let replace_size = replace_with.len() as u64;
    if replace_size > MAX_FILE_SIZE {
        return (
            format!(
                "Error: Replacement text too large ({} bytes, max is {} bytes)",
                replace_size, MAX_FILE_SIZE
            ),
            false,
        );
    }
    match Regex::new(pattern) {
        Ok(re) => {
            match fs::metadata(file_path) {
                Ok(metadata) if metadata.len() > MAX_FILE_SIZE => {
                    return (
                        format!(
                            "Error: File '{}' is too large ({} bytes, max is {} bytes)",
                            filename,
                            metadata.len(),
                            MAX_FILE_SIZE
                        ),
                        false,
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    return (
                        format!("Error reading file metadata '{}': {}", filename, e),
                        false,
                    )
                }
            }
            match fs::read_to_string(file_path) {
                Ok(content) => {
                    let new_content = re.replace_all(&content, replace_with);
                    if let Err(msg) = confirm_and_apply_change(
                        &content,
                        &new_content,
                        filename,
                        "search and replace in",
                        skip_confirmation,
                    ) {
                        let is_cancel = msg == CANCELLATION_MESSAGE;
                        return (msg, is_cancel);
                    }
                    let result_size = new_content.len() as u64;
                    if result_size > MAX_FILE_SIZE {
                        return (
                            format!(
                                "Error: Resulting content too large ({} bytes, max is {} bytes)",
                                result_size, MAX_FILE_SIZE
                            ),
                            false,
                        );
                    }
                    match fs::write(file_path, new_content.as_ref()) {
                        Ok(()) => (
                            format!(
                                "Successfully replaced pattern '{}' with '{}' in '{}'",
                                pattern, replace_with, filename
                            ),
                            false,
                        ),
                        Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
                    }
                }
                Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
            }
        }
        Err(e) => (
            format!("Error compiling regex pattern '{}': {}", pattern, e),
            false,
        ),
    }
}

fn handle_apply_diff(
    file_path: &PathBuf,
    filename: &str,
    data: Option<&str>,
    skip_confirmation: bool,
) -> (String, bool) {
    let diff_content = match data {
        Some(d) => d,
        None => {
            return (
                "Error: 'data' parameter with diff content is required for apply_diff".to_string(),
                false,
            );
        }
    };

    match fs::read_to_string(file_path) {
        Ok(original_content) => match apply_patch(&original_content, diff_content) {
            Ok(new_content) => {
                if let Err(msg) = confirm_and_apply_change(
                    &original_content,
                    &new_content,
                    filename,
                    "applying diff to",
                    skip_confirmation,
                ) {
                    let is_cancel = msg == CANCELLATION_MESSAGE;
                    return (msg, is_cancel);
                }
                match fs::write(file_path, &new_content) {
                    Ok(()) => (
                        format!("Successfully applied diff to '{}'", filename),
                        false,
                    ),
                    Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
                }
            }
            Err(e) => (format!("Error parsing or applying diff: {}", e), false),
        },
        Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
    }
}

pub fn file_editor(
    subcommand: &str,
    filename: &str,
    data: Option<&str>,
    replacement: Option<&str>,
    skip_confirmation: bool,
    debug: bool,
) -> (String, bool) {
    let file_path = match resolve_sandbox_path(filename) {
        Ok(p) => p,
        Err(e) => return (e, false),
    };

    crate::utils::log_to_file(
        debug,
        &format!(
            "File editor: subcommand={}, filename={}",
            subcommand, filename
        ),
    );

    let (result, rejected) = match subcommand {
        "read" => handle_read(&file_path, filename),
        "write" => handle_write(&file_path, filename, data, skip_confirmation),
        "search" => handle_search(&file_path, filename, data),
        "search_and_replace" => {
            handle_search_and_replace(&file_path, filename, data, replacement, skip_confirmation)
        }
        "apply_diff" => handle_apply_diff(&file_path, filename, data, skip_confirmation),
        _ => (format!("Error: Unknown subcommand '{}'", subcommand), false),
    };

    crate::utils::log_to_file(debug, &format!("File editor result: {}", result));

    (result, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_path_traversal_parent() {
        let result = resolve_sandbox_path("../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("traversal") || err.contains("denied"));
    }

    #[test]
    fn test_reject_path_traversal_nested() {
        let result = resolve_sandbox_path("foo/../../etc/shadow");
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_absolute_path() {
        let result = resolve_sandbox_path("/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_accept_simple_filename() {
        let test_file = "test_resolve_sandbox_path.tmp";
        std::fs::write(test_file, "test").unwrap();
        let result = resolve_sandbox_path(test_file);
        assert!(result.is_ok());
        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_accept_subdirectory() {
        let result = resolve_sandbox_path("Cargo.toml");
        assert!(result.is_ok());
    }

    #[test]
    fn test_max_file_size_constant() {
        assert_eq!(MAX_FILE_SIZE, 10 * 1024 * 1024);
    }

    // --- Integration-style tests with real filesystem ---

    #[test]
    fn test_integration_path_traversal_parent_dir_blocked() {
        let (result, rejected) = file_editor("read", "../../etc/passwd", None, None, true, false);
        assert!(
            result.contains("traversal") || result.contains("denied"),
            "Expected traversal/denied error, got: {}",
            result
        );
        assert!(!rejected);
    }

    #[test]
    fn test_integration_path_traversal_nested_blocked() {
        let (result, _) = file_editor("read", "foo/../../etc/shadow", None, None, true, false);
        assert!(
            result.contains("traversal") || result.contains("denied"),
            "Expected traversal/denied error, got: {}",
            result
        );
    }

    #[test]
    fn test_integration_absolute_path_blocked() {
        let (result, _) = file_editor("read", "/etc/passwd", None, None, true, false);
        assert!(
            result.contains("traversal")
                || result.contains("denied")
                || result.contains("outside sandbox"),
            "Expected access denied error, got: {}",
            result
        );
    }

    #[test]
    fn test_integration_read_existing_file() {
        let test_file = "test_integration_read_existing.tmp";
        fs::write(test_file, "hello integration test").unwrap();
        let (result, rejected) = file_editor("read", test_file, None, None, true, false);
        let _ = fs::remove_file(test_file);
        assert!(!rejected);
        assert!(result.contains("hello integration test"));
    }

    #[test]
    fn test_integration_write_and_read_back() {
        let test_file = "test_integration_write_read.tmp";
        fs::write(test_file, "").unwrap();
        let (result, rejected) = file_editor(
            "write",
            test_file,
            Some("integration write content"),
            None,
            true,
            false,
        );
        assert!(!rejected);
        assert!(result.contains("Successfully wrote"));
        let content = fs::read_to_string(test_file).unwrap();
        assert_eq!(content, "integration write content");
        let _ = fs::remove_file(test_file);
    }

    #[test]
    fn test_integration_search_in_file() {
        let test_file = "test_integration_search.tmp";
        fs::write(test_file, "line one\nline two\nline three").unwrap();
        let (result, _) = file_editor("search", test_file, Some("line two"), None, true, false);
        let _ = fs::remove_file(test_file);
        assert!(result.contains("Found 1 match"));
    }

    #[test]
    fn test_integration_read_nonexistent_file() {
        let (result, _) = file_editor(
            "read",
            "nonexistent_integration_test_xyz.tmp",
            None,
            None,
            true,
            false,
        );
        assert!(
            result.contains("Failed to resolve") || result.contains("Error"),
            "Expected error for nonexistent file, got: {}",
            result
        );
    }

    #[test]
    fn test_integration_create_subdirectory_and_write() {
        let test_dir = "test_integration_subdir";
        let test_file = "test_integration_subdir/nested_file.tmp";
        let _ = fs::remove_dir_all(test_dir);
        fs::create_dir_all(test_dir).unwrap();
        fs::write(test_file, "").unwrap();
        let (result, rejected) = file_editor(
            "write",
            test_file,
            Some("nested content"),
            None,
            true,
            false,
        );
        assert!(!rejected);
        assert!(result.contains("Successfully wrote"));
        let content = fs::read_to_string(test_file).unwrap();
        assert_eq!(content, "nested content");
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_integration_symlink_to_sandbox_file() {
        let target_file = "test_symlink_target.tmp";
        let link_file = "test_symlink_link.tmp";
        fs::write(target_file, "symlink target content").unwrap();
        let _ = fs::remove_file(link_file);
        use std::os::unix::fs::symlink;
        symlink(target_file, link_file).unwrap();
        let (result, _) = file_editor("read", link_file, None, None, true, false);
        let _ = fs::remove_file(link_file);
        let _ = fs::remove_file(target_file);
        assert!(result.contains("symlink target content"));
    }
}
