use regex::Regex;
use std::fs;
use std::path::PathBuf;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use difference::{Changeset, Difference};
use crate::sandbox::get_sandbox_root;

use crate::patch::apply_patch;

fn confirm_change(original: &str, new_content: &str, filename: &str, operation_desc: &str) -> Result<bool, String> {
    let changeset = Changeset::new(original, new_content, "\n");
    println!("Diff preview for {} in '{}':", operation_desc, filename);
    for diff in &changeset.diffs {
        match diff {
            Difference::Same(ref s) => println!("\x1b[2m {}\x1b[0m", s),
            Difference::Rem(ref s) => println!("\x1b[91m-{}\x1b[0m", s),
            Difference::Add(ref s) => println!("\x1b[92m+{}\x1b[0m", s),
        }
    }
    println!("Press Enter to apply, Escape to cancel");
    let stdin_fd = io::stdin().as_raw_fd();
    let mut orig_term: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { libc::tcgetattr(stdin_fd, &mut orig_term) };
    let mut raw_term = orig_term;
    unsafe { libc::cfmakeraw(&mut raw_term) };
    unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &raw_term) };
    let confirmed = loop {
        let mut buf = [0u8; 1];
        if io::stdin().read_exact(&mut buf).is_ok() {
            let c = buf[0];
            if c == b'\r' { // Enter
                break true;
            } else if c == 0x1b || c == 0x03 { // Escape or ^C
                break false;
            }
        } else {
            break false;
        }
    };
    unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &orig_term) };
    Ok(confirmed)
}

pub fn file_editor(
    subcommand: &str,
    filename: &str,
    data: Option<&str>,
    replacement: Option<&str>,
    skip_confirmation: bool,
) -> String {
    let file_path = PathBuf::from(get_sandbox_root()).join(filename);

    match subcommand {
        "read" => match fs::read_to_string(&file_path) {
            Ok(content) => format!("File contents:\n{}", content),
            Err(e) => format!("Error reading file '{}': {}", filename, e),
        },
        "write" => {
            let new_content = data.unwrap_or("");
            if !skip_confirmation {
                let current_content = fs::read_to_string(&file_path).unwrap_or_default();
                match confirm_change(&current_content, new_content, filename, "writing to") {
                    Ok(true) => {},
                                    Ok(false) => return "User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(),
                    Err(e) => return e,
                }
            }
            match fs::write(&file_path, new_content) {
                Ok(()) => format!("Successfully wrote to '{}'", filename),
                Err(e) => format!("Error writing to '{}': {}", filename, e),
            }
        }
        "search" => {
            let pattern = match data {
                Some(p) => p,
                None => {
                    return "Error: 'data' parameter with regex pattern is required for search"
                        .to_string()
                }
            };
            match Regex::new(pattern) {
                Ok(re) => match fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let matches: Vec<_> = re.find_iter(&content).collect();
                        if matches.is_empty() {
                            format!(
                                "No matches found for pattern '{}' in '{}'",
                                pattern, filename
                            )
                        } else {
                            let match_list: Vec<String> = matches
                                .iter()
                                .map(|m| format!(" - {} (at position {})", m.as_str(), m.start()))
                                .collect();
                            format!(
                                "Found {} matches for pattern '{}' in '{}':\n{}",
                                matches.len(),
                                pattern,
                                filename,
                                match_list.join("\n")
                            )
                        }
                    }
                    Err(e) => format!("Error reading file '{}': {}", filename, e),
                },
                Err(e) => format!("Error compiling regex pattern '{}': {}", pattern, e),
            }
        }
        "search_and_replace" => {
            let pattern = match data {
                Some(p) => p,
                None => return "Error: 'data' parameter with regex pattern is required for search_and_replace".to_string(),
            };
            let replace_with = match replacement {
                Some(r) => r,
                None => {
                    return "Error: 'replacement' parameter is required for search_and_replace"
                        .to_string()
                }
            };
            match Regex::new(pattern) {
                Ok(re) => match fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let new_content = re.replace_all(&content, replace_with);
                        if !skip_confirmation {
                            match confirm_change(&content, &new_content, filename, "search and replace in") {
                                Ok(true) => {},
                                Ok(false) => return "User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(),
                                Err(e) => return e,
                            }
                        }
                        match fs::write(&file_path, new_content.as_ref()) {
                            Ok(()) => format!(
                                "Successfully replaced pattern '{}' with '{}' in '{}'",
                                pattern, replace_with, filename
                            ),
                            Err(e) => format!("Error writing to '{}': {}", filename, e),
                        }
                    }
                    Err(e) => format!("Error reading file '{}': {}", filename, e),
                },
                Err(e) => format!("Error compiling regex pattern '{}': {}", pattern, e),
            }
        }
        "apply_diff" => {
            let diff_content = match data {
                Some(d) => d,
                None => {
                    return "Error: 'data' parameter with diff content is required for apply_diff"
                        .to_string()
                }
            };

            match fs::read_to_string(&file_path) {
                Ok(original_content) => {
                    // Parse and apply the diff
                    match apply_patch(&original_content, diff_content) {
                        Ok(new_content) => {
                            if !skip_confirmation {
                                match confirm_change(&original_content, &new_content, filename, "applying diff to") {
                                    Ok(true) => {},
                    Ok(false) => return "User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(),
                                    Err(e) => return e,
                                }
                            }
                            // Write the new content back to the file
                            match fs::write(&file_path, &new_content) {
                                Ok(()) => format!("Successfully applied diff to '{}'", filename),
                                Err(e) => format!("Error writing to '{}': {}", filename, e),
                            }
                        },
                        Err(e) => format!("Error parsing or applying diff: {}", e),
                    }
                }
                Err(e) => format!("Error reading file '{}': {}", filename, e),
            }
        }
        _ => format!("Error: Unknown subcommand '{}'", subcommand),
    }
}

