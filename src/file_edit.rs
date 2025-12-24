use regex::Regex;
use std::fs;
use std::path::PathBuf;
use difference::{Changeset, Difference};

use crate::sandbox::get_sandbox_root;
use anyhow::Result;

use crate::patch::apply_patch;

const CANCELLATION_MESSAGE: &str = "User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.";

fn confirm_change(original: &str, new_content: &str, filename: &str, operation_desc: &str) -> Result<bool> {
    let changeset = Changeset::new(original, new_content, "\n");
    println!("Diff preview for {} in '{}':", operation_desc, filename);
    for diff in &changeset.diffs {
        match diff {
            Difference::Same(ref s) => println!("\x1b[2m {}\x1b[0m", s),
            Difference::Rem(ref s) => println!("\x1b[91m-{}\x1b[0m", s),
            Difference::Add(ref s) => println!("\x1b[92m+{}\x1b[0m", s),
        }
    }
    let confirmed = dialoguer::Confirm::new()
        .with_prompt("Apply changes?")
        .default(false)
        .interact()
        .unwrap_or(false);
    Ok(confirmed)
}

fn confirm_and_apply_change(old_content: &str, new_content: &str, filename: &str, operation_desc: &str, skip_confirmation: bool) -> Result<(), String> {
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
    match fs::read_to_string(file_path) {
        Ok(content) => (format!("File contents:\n{}", content), false),
        Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
    }
}

fn handle_write(file_path: &PathBuf, filename: &str, data: Option<&str>, skip_confirmation: bool) -> (String, bool) {
    let new_content = data.unwrap_or("");
    let current_content = fs::read_to_string(file_path).unwrap_or_default();
    if let Err(msg) = confirm_and_apply_change(&current_content, new_content, filename, "writing to", skip_confirmation) {
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
            return ("Error: 'data' parameter with regex pattern is required for search".to_string(), false);
        }
    };
    match Regex::new(pattern) {
        Ok(re) => match fs::read_to_string(file_path) {
            Ok(content) => {
                let matches: Vec<_> = re.find_iter(&content).collect();
                if matches.is_empty() {
                    (format!("No matches found for pattern '{}' in '{}'", pattern, filename), false)
                } else {
                    let match_list: Vec<String> = matches
                        .iter()
                        .map(|m| format!(" - {} (at position {})", m.as_str(), m.start()))
                        .collect();
                    (format!(
                        "Found {} matches for pattern '{}' in '{}':\n{}",
                        matches.len(),
                        pattern,
                        filename,
                        match_list.join("\n")
                    ), false)
                }
            }
            Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
        },
        Err(e) => (format!("Error compiling regex pattern '{}': {}", pattern, e), false),
    }
}

fn handle_search_and_replace(file_path: &PathBuf, filename: &str, data: Option<&str>, replacement: Option<&str>, skip_confirmation: bool) -> (String, bool) {
    let pattern = match data {
        Some(p) => p,
        None => return ("Error: 'data' parameter with regex pattern is required for search_and_replace".to_string(), false),
    };
    let replace_with = match replacement {
        Some(r) => r,
        None => {
            return ("Error: 'replacement' parameter is required for search_and_replace".to_string(), false);
        }
    };
    match Regex::new(pattern) {
        Ok(re) => match fs::read_to_string(file_path) {
            Ok(content) => {
                let new_content = re.replace_all(&content, replace_with);
                if let Err(msg) = confirm_and_apply_change(&content, &new_content, filename, "search and replace in", skip_confirmation) {
                    let is_cancel = msg == CANCELLATION_MESSAGE;
                    return (msg, is_cancel);
                }
                match fs::write(file_path, new_content.as_ref()) {
                    Ok(()) => (format!(
                        "Successfully replaced pattern '{}' with '{}' in '{}'",
                        pattern, replace_with, filename
                    ), false),
                    Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
                }
            }
            Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
        },
        Err(e) => (format!("Error compiling regex pattern '{}': {}", pattern, e), false),
    }
}

fn handle_apply_diff(file_path: &PathBuf, filename: &str, data: Option<&str>, skip_confirmation: bool) -> (String, bool) {
    let diff_content = match data {
        Some(d) => d,
        None => {
            return ("Error: 'data' parameter with diff content is required for apply_diff".to_string(), false);
        }
    };

    match fs::read_to_string(file_path) {
        Ok(original_content) => {
            match apply_patch(&original_content, diff_content) {
                Ok(new_content) => {
                    if let Err(msg) = confirm_and_apply_change(&original_content, &new_content, filename, "applying diff to", skip_confirmation) {
                        let is_cancel = msg == CANCELLATION_MESSAGE;
                        return (msg, is_cancel);
                    }
                    match fs::write(file_path, &new_content) {
                        Ok(()) => (format!("Successfully applied diff to '{}'", filename), false),
                        Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
                    }
                },
                Err(e) => (format!("Error parsing or applying diff: {}", e), false),
            }
        }
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
    let file_path = PathBuf::from(get_sandbox_root()).join(filename);

    crate::log_to_file(debug, &format!("File editor: subcommand={}, filename={}", subcommand, filename));

    let (result, rejected) = match subcommand {
        "read" => handle_read(&file_path, filename),
        "write" => handle_write(&file_path, filename, data, skip_confirmation),
        "search" => handle_search(&file_path, filename, data),
        "search_and_replace" => handle_search_and_replace(&file_path, filename, data, replacement, skip_confirmation),
        "apply_diff" => handle_apply_diff(&file_path, filename, data, skip_confirmation),
        _ => (format!("Error: Unknown subcommand '{}'", subcommand), false),
    };

    crate::log_to_file(debug, &format!("File editor result: {}", result));

    (result, rejected)
}

