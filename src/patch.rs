use regex::Regex;

pub fn apply_patch(original: &str, diff: &str) -> Result<String, String> {
    let original_lines: Vec<&str> = original.lines().collect();
    let mut result_lines = original_lines.clone();

    // Track line offset due to previous hunks' changes
    let mut line_offset: i32 = 0;
    let mut current_section_start_line = 0;
    let mut in_hunk = false;
    let mut hunk_additions: i32 = 0;
    let mut hunk_removals: i32 = 0;

    // Regular expression for unified diff hunk headers: @@ -a,b +c,d @@
    let hunk_header_re = Regex::new(r"@@ -(\d+),\d+ \+(\d+),\d+ @@").map_err(|e| e.to_string())?;

    // Process the diff line by line
    for line in diff.lines() {
        // Check if this is a hunk header line
        if let Some(caps) = hunk_header_re.captures(line) {
            // Apply offset from previous hunk
            line_offset += hunk_additions - hunk_removals;
            hunk_additions = 0;
            hunk_removals = 0;
            in_hunk = true;

            // Parse the original start line
            let original_start: usize = caps[1].parse().map_err(|e| format!("Invalid line number '{}': {}", &caps[1], e))?;

            // Adjust for offset
            current_section_start_line = (original_start as i32 - 1 + line_offset) as usize;
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
                    hunk_additions += 1;
                },
                Some('-') => {
                    // Removal line: remove at current position
                    if current_section_start_line < result_lines.len() {
                        result_lines.remove(current_section_start_line);
                    } else {
                        return Err(format!("Diff removal line {} is out of bounds", current_section_start_line));
                    }
                    hunk_removals += 1;
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