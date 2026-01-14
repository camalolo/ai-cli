use anyhow::Result;

use crate::sandbox::get_sandbox_root;

pub fn execute_command(command: &str, debug: bool) -> Result<String> {
    if command.trim().is_empty() {
        return Ok("Error: No command provided".to_string());
    }

    crate::utils::log_to_file(debug, &format!("Executing command: {}", command));

    #[cfg(target_os = "linux")]
    let output = execute_with_bubblewrap(command, debug)?;

    #[cfg(not(target_os = "linux"))]
    let output = execute_without_sandbox(command, debug)?;

    crate::utils::log_to_file(debug, &format!("Command result: {}", output));

    Ok(output)
}

#[cfg(target_os = "linux")]
fn execute_with_bubblewrap(command: &str, debug: bool) -> Result<String> {
    let sandbox_root = get_sandbox_root();

    crate::utils::log_to_file(debug, &format!("Sandbox root: {}", sandbox_root));

    let output = std::process::Command::new("bwrap")
        .args([
            "--ro-bind", "/", "/",
            "--bind", sandbox_root, sandbox_root,
            "--dev", "/dev",
            "--proc", "/proc",
            "--die-with-parent",
            "/bin/sh", "-c", command,
        ])
        .current_dir(sandbox_root)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run bwrap: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let result = if output.status.success() {
        if stdout.is_empty() && stderr.is_empty() {
            "Command executed (no output)".to_string()
        } else {
            format!("{}{}", stdout, stderr)
        }
    } else {
        format!("Command '{}' exited with non-zero status: {}", command, stderr.trim())
    };

    Ok(result)
}

#[cfg(not(target_os = "linux"))]
fn execute_without_sandbox(command: &str, debug: bool) -> Result<String> {
    let parsed: Vec<String> = shell_words::split(command)
        .map_err(|e| anyhow::anyhow!("Failed to parse command: {}", e))?;

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

    Ok(result)
}

