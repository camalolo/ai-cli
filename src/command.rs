use anyhow::Result;

use crate::sandbox::get_sandbox_root;

pub fn execute_command(command: &str) -> Result<String> {
    if command.trim().is_empty() {
        return Ok("Error: No command provided".to_string());
    }

    let parsed: Vec<String> = shell_words::split(command)
        .map_err(|e| anyhow::anyhow!("Failed to parse command: {}", e))?;

    if parsed.is_empty() {
        return Ok("Error: No command provided".to_string());
    }

    let output = std::process::Command::new(&parsed[0])
        .args(&parsed[1..])
        .current_dir(get_sandbox_root())
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        if stdout.is_empty() && stderr.is_empty() {
            Ok("Command executed (no output)".to_string())
        } else {
            Ok(format!("{}{}", stdout, stderr))
        }
    } else {
        Ok(format!("Command '{}' exited with non-zero status", command))
    }
}

