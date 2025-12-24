use anyhow::Result;

use crate::sandbox::get_sandbox_root;

pub fn execute_command(command: &str, debug: bool) -> Result<String> {
    if command.trim().is_empty() {
        return Ok("Error: No command provided".to_string());
    }

    let parsed: Vec<String> = shell_words::split(command)
        .map_err(|e| anyhow::anyhow!("Failed to parse command: {}", e))?;

    crate::log_to_file(debug, &format!("Executing command: {}", command));

    let output = std::process::Command::new(&parsed[0])
        .args(&parsed[1..])
        .current_dir(get_sandbox_root())
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let result = if output.status.success() {
        if stdout.is_empty() && stderr.is_empty() {
            "Command executed (no output)".to_string()
        } else {
            format!("{}{}", stdout, stderr)
        }
    } else {
        format!("Command '{}' exited with non-zero status", command)
    };

    crate::log_to_file(debug, &format!("Command result: {}", result));

    Ok(result)
}

