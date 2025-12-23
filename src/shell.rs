use colored::{Color, Colorize};
use rustyline::DefaultEditor;
use std::env;
use std::process::Command;
use crate::command::execute_command;

fn get_version_line(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd).args(args).output().ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8_lossy(&output.stdout).lines().next().map(|s| s.to_string()))
}

pub fn detect_shell_info() -> String {
    if cfg!(target_os = "windows") {
        detect_windows_shell()
    } else {
        detect_unix_shell()
    }
}

fn detect_windows_shell() -> String {
    // Check for MSYS/MINGW environments first (Git Bash, MSYS2, etc.)
    if let Ok(msystem) = env::var("MSYSTEM") {
        if !msystem.is_empty() {
            // We're in a MSYS/MINGW environment (Git Bash, MSYS2, etc.)
            let system_name = match msystem.as_str() {
                "MINGW64" => "Git Bash (MINGW64)",
                "MINGW32" => "Git Bash (MINGW32)",
                "MSYS" => "MSYS",
                _ => "MSYS/MINGW",
            };

            // Try to get bash version
            if let Some(version) = get_version_line("bash", &["--version"]) {
                return format!("{} - {}", system_name, version);
            }
            return system_name.to_string();
        }
    }

    // Check if we're running under bash (could be Git Bash without MSYSTEM set)
    if let Ok(shell) = env::var("SHELL") {
        if shell.contains("bash") || shell.contains("sh") {
            // Try to get bash version
            if let Some(version) = get_version_line("bash", &["--version"]) {
                return format!("Git Bash - {}", version);
            }
            return "Git Bash".to_string();
        }
    }

    // Check for PowerShell
    if let Ok(powershell_path) = env::var("PSModulePath") {
        if !powershell_path.is_empty() {
            // Try to get PowerShell version
            if let Some(version) = get_version_line("powershell", &["-Command", "$PSVersionTable.PSVersion.ToString()"]) {
                return format!("PowerShell {}", version.trim());
            }
            return "PowerShell".to_string();
        }
    }

    // Default to cmd.exe
    "Command Prompt (cmd.exe)".to_string()
}

fn detect_unix_shell() -> String {
    // On Unix-like systems, use SHELL environment variable
    if let Ok(shell_path) = env::var("SHELL") {
        // Extract shell name from path
        let shell_name = std::path::Path::new(&shell_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bash");

        // Try to get version for common shells
        let version_cmd = match shell_name {
            "bash" => Some(("bash", vec!["--version"])),
            "zsh" => Some(("zsh", vec!["--version"])),
            "fish" => Some(("fish", vec!["--version"])),
            "tcsh" | "csh" => Some((shell_name, vec!["--version"])),
            "ksh" => Some((shell_name, vec!["--version"])),
            _ => None,
        };

        if let Some((cmd, args)) = version_cmd {
            if let Some(version) = get_version_line(cmd, &args) {
                return version;
            }
        }

        // Fallback to shell name
        shell_name.to_string()
    } else {
        "bash".to_string()
    }
}

pub fn interactive_shell(_debug: bool) -> String {
    println!("{}", "Entering interactive shell mode. Type 'exit' to return.".color(Color::Cyan));
    let mut accumulated_output = String::new();
    let mut rl = DefaultEditor::new().expect("Failed to create editor");
    loop {
        let readline = rl.readline("shell> ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input == "exit" {
                    break;
                }
                rl.add_history_entry(input).ok();
                let output = execute_command(input).unwrap_or_else(|e| e.to_string());
                println!("{}", output.color(Color::Magenta));
                accumulated_output.push_str(&format!("Command: {}\nOutput: {}\n\n", input, output));
            }
            Err(_) => break,
        }
    }
    println!("{}", "Exiting interactive shell mode.".color(Color::Cyan));
    accumulated_output
}