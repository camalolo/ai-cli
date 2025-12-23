use regex::Regex;

pub fn apply_patch(original: &str, diff: &str) -> Result<String, String> {
    let original_lines: Vec<&str> = original.lines().collect();
    let mut result_lines = original_lines.clone();
    
    // Current position in the parsing of the diff
    let mut current_section_start_line = 0;
    let mut in_hunk = false;
    
    // Regular expression for unified diff hunk headers: @@ -a,b +c,d @@
    let hunk_header_re = Regex::new(r"@@ -(\d+),(\d+) \+(\d+),(\d+) @@").map_err(|e| e.to_string())?;
    
    // Process the diff line by line
    for line in diff.lines() {
        // Check if this is a hunk header line
        if let Some(caps) = hunk_header_re.captures(line) {
            in_hunk = true;
            
            // Parse the line numbers and counts from the hunk header
            let original_start: usize = caps[1].parse().map_err(|_| "Invalid line number in diff".to_string())?;
            let _original_count: usize = caps[2].parse().map_err(|_| "Invalid line count in diff".to_string())?;
            
            // In unified diffs, line numbers are 1-based, so we subtract 1 for 0-based indexing
            current_section_start_line = original_start - 1;
            continue;
        }
        
        // Skip file header lines in unified diff
        if line.starts_with("---") || line.starts_with("+++") {
            continue;
        }
        
        // If we're in a hunk, process addition/removal/context lines
        if in_hunk {
            match line.chars().next() {
                Some('+') => {
                    // Addition line: insert at current position
                    let content = &line[1..]; // Skip the '+' prefix
                    result_lines.insert(current_section_start_line, content);
                    current_section_start_line += 1;
                },
                Some('-') => {
                    // Removal line: remove at current position
                    if current_section_start_line < result_lines.len() {
                        result_lines.remove(current_section_start_line);
                    } else {
                        return Err(format!("Diff removal line {} is out of bounds", current_section_start_line));
                    }
                },
                Some(' ') => {
                    // Context line: just advance position
                    current_section_start_line += 1;
                },
                _ => {
                    // Other lines in the diff (could be comments, etc.)
                    // Ignore them
                }
            }
        }
    }
    
    Ok(result_lines.join("\n"))
}