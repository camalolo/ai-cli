use colored::{Color, Colorize};
use std::io::{self, Write};
use std::env;
use std::process::Command;
use crate::command::execute_command;

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
            if let Ok(version_output) = Command::new("bash")
                .arg("--version")
                .output()
            {
                if version_output.status.success() {
                    let output = String::from_utf8_lossy(&version_output.stdout);
                    if let Some(first_line) = output.lines().next() {
                        return format!("{} - {}", system_name, first_line);
                    }
                }
            }
            return system_name.to_string();
        }
    }

    // Check if we're running under bash (could be Git Bash without MSYSTEM set)
    if let Ok(shell) = env::var("SHELL") {
        if shell.contains("bash") || shell.contains("sh") {
            // Try to get bash version
            if let Ok(version_output) = Command::new("bash")
                .arg("--version")
                .output()
            {
                if version_output.status.success() {
                    let output = String::from_utf8_lossy(&version_output.stdout);
                    if let Some(first_line) = output.lines().next() {
                        return format!("Git Bash - {}", first_line);
                    }
                }
            }
            return "Git Bash".to_string();
        }
    }

    // Check for PowerShell
    if let Ok(powershell_path) = env::var("PSModulePath") {
        if !powershell_path.is_empty() {
            // Try to get PowerShell version
            if let Ok(version_output) = Command::new("powershell")
                .arg("-Command")
                .arg("$PSVersionTable.PSVersion.ToString()")
                .output()
            {
                if version_output.status.success() {
                    let version = String::from_utf8_lossy(&version_output.stdout).trim().to_string();
                    return format!("PowerShell {}", version);
                }
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
            if let Ok(version_output) = Command::new(cmd)
                .args(&args)
                .output()
            {
                if version_output.status.success() {
                    let output = String::from_utf8_lossy(&version_output.stdout);
                    if let Some(first_line) = output.lines().next() {
                        return first_line.to_string();
                    }
                }
            }
        }

        // Fallback to shell name
        shell_name.to_string()
    } else {
        "bash".to_string()
    }
}

pub fn interactive_shell() -> String {
    println!("{}", "Entering interactive shell mode. Type 'exit' to return.".color(Color::Cyan));
    let mut accumulated_output = String::new();
    loop {
        print!("shell> ");
        io::stdout().flush().expect("Failed to flush stdout");

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim();
                if input == "exit" {
                    break;
                }
                let output = execute_command(input);
                println!("{}", output.color(Color::Magenta));
                accumulated_output.push_str(&format!("Command: {}\nOutput: {}\n\n", input, output));
            }
            Err(e) => {
                println!("{}", format!("Input error: {}", e).color(Color::Red));
                break;
            }
        }
    }
    println!("{}", "Exiting interactive shell mode.".color(Color::Cyan));
    accumulated_output
}