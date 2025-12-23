use regex::Regex;
use std::fs;
use std::path::PathBuf;
use difference::{Changeset, Difference};
use crossterm::{terminal, event::{self, KeyCode}};
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
    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let confirmed = loop {
        match event::read() {
            Ok(event::Event::Key(key_event)) => match key_event.code {
                KeyCode::Enter => break Ok(true),
                KeyCode::Esc => break Ok(false),
                _ => {}
            },
            _ => break Err("Failed to read input".to_string()),
        }
    };
    terminal::disable_raw_mode().ok();
    confirmed
}

pub fn file_editor(
    subcommand: &str,
    filename: &str,
    data: Option<&str>,
    replacement: Option<&str>,
    skip_confirmation: bool,
) -> (String, bool) {
    let file_path = PathBuf::from(get_sandbox_root()).join(filename);

    match subcommand {
        "read" => match fs::read_to_string(&file_path) {
            Ok(content) => (format!("File contents:\n{}", content), false),
            Err(e) => (format!("Error reading file '{}': {}", filename, e), false),
        },
        "write" => {
            let new_content = data.unwrap_or("");
            if !skip_confirmation {
                let current_content = fs::read_to_string(&file_path).unwrap_or_default();
                match confirm_change(&current_content, new_content, filename, "writing to") {
                    Ok(true) => {},
                                    Ok(false) => return ("User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(), true),
                    Err(e) => return (e, false),
                }
            }
            match fs::write(&file_path, new_content) {
                Ok(()) => (format!("Successfully wrote to '{}'", filename), false),
                Err(e) => (format!("Error writing to '{}': {}", filename, e), false),
            }
        }
        "search" => {
            let pattern = match data {
                Some(p) => p,
                None => {
                    return ("Error: 'data' parameter with regex pattern is required for search"
                        .to_string(), false)
                }
            };
             match Regex::new(pattern) {
                 Ok(re) => match fs::read_to_string(&file_path) {
                     Ok(content) => {
                         let matches: Vec<_> = re.find_iter(&content).collect();
                         if matches.is_empty() {
                             (format!(
                                 "No matches found for pattern '{}' in '{}'",
                                 pattern, filename
                             ), false)
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
        "search_and_replace" => {
            let pattern = match data {
                Some(p) => p,
                None => return ("Error: 'data' parameter with regex pattern is required for search_and_replace".to_string(), false),
            };
            let replace_with = match replacement {
                Some(r) => r,
                None => {
                    return ("Error: 'replacement' parameter is required for search_and_replace"
                        .to_string(), false)
                }
            };
            match Regex::new(pattern) {
                Ok(re) => match fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let new_content = re.replace_all(&content, replace_with);
                        if !skip_confirmation {
                            match confirm_change(&content, &new_content, filename, "search and replace in") {
                                Ok(true) => {},
                                Ok(false) => return ("User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(), true),
                                Err(e) => return (e, false),
                            }
                        }
                        match fs::write(&file_path, new_content.as_ref()) {
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
        "apply_diff" => {
            let diff_content = match data {
                Some(d) => d,
                None => {
                    return ("Error: 'data' parameter with diff content is required for apply_diff"
                        .to_string(), false)
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
                    Ok(false) => return ("User has cancelled this operation because it is against their wishes. Do not attempt any alternative approaches or modifications. Wait for further instructions.".to_string(), true),
                                    Err(e) => return (e, false),
                                }
                            }
                            // Write the new content back to the file
                            match fs::write(&file_path, &new_content) {
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
        _ => (format!("Error: Unknown subcommand '{}'", subcommand), false),
    }
}

