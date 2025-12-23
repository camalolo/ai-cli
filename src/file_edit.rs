use regex::Regex;
use std::fs;
use std::path::PathBuf;
use difference::Changeset;
use crate::sandbox::SANDBOX_ROOT;

use crate::patch::apply_patch;

pub fn file_editor(
    subcommand: &str,
    filename: &str,
    data: Option<&str>,
    replacement: Option<&str>,
    skip_confirmation: bool,
) -> String {
    let file_path = PathBuf::from(&*SANDBOX_ROOT).join(filename);

    match subcommand {
        "read" => match fs::read_to_string(&file_path) {
            Ok(content) => format!("File contents:\n{}", content),
            Err(e) => format!("Error reading file '{}': {}", filename, e),
        },
        "write" => {
            let new_content = data.unwrap_or("");
            if !skip_confirmation {
                let current_content = fs::read_to_string(&file_path).unwrap_or_default();
                let changeset = Changeset::new(&current_content, new_content, "\n");
                println!("Diff preview for writing to '{}':", filename);
                println!("{}", changeset);
                println!("Apply this change? (y/n)");
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err() {
                    return "Failed to read user input".to_string();
                }
                if !input.trim().to_lowercase().starts_with('y') {
                    return "Write operation cancelled by user".to_string();
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
                            let changeset = Changeset::new(&content, &new_content, "\n");
                            println!("Diff preview for search and replace in '{}':", filename);
                            println!("{}", changeset);
                            println!("Apply this change? (y/n)");
                            let mut input = String::new();
                            if std::io::stdin().read_line(&mut input).is_err() {
                                return "Failed to read user input".to_string();
                            }
                            if !input.trim().to_lowercase().starts_with('y') {
                                return "Search and replace operation cancelled by user".to_string();
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
                                let changeset = Changeset::new(&original_content, &new_content, "\n");
                                println!("Diff preview for applying diff to '{}':", filename);
                                println!("{}", changeset);
                                println!("Apply this change? (y/n)");
                                let mut input = String::new();
                                if std::io::stdin().read_line(&mut input).is_err() {
                                    return "Failed to read user input".to_string();
                                }
                                if !input.trim().to_lowercase().starts_with('y') {
                                    return "Apply diff operation cancelled by user".to_string();
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

