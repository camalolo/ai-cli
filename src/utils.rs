use colored::{Color, Colorize};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

pub fn print_error(message: &str) {
    println!("{}", message.color(Color::Red));
}

pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

pub fn log_to_file(debug: bool, msg: &str) {
    if debug {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("debug.log")
        {
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }
}

pub fn clear_debug_file(debug: bool) {
    if debug {
        if let Err(e) = std::fs::File::create("debug.log") {
            eprintln!("Warning: Failed to clear debug.log: {}", e);
        }
    }
}

pub fn get_opt_str(args: &Value, key: &str, default: &str) -> String {
    args.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string()
}

pub fn get_opt_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

pub fn confirm(prompt: &str) -> bool {
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .unwrap_or(false)
}

/// Prompts user with y/n/a options (yes/no/always)
/// Returns (confirmed, always) tuple where:
/// - confirmed: true if user selected yes or always
/// - always: true if user selected always (approve all commands for this session)
pub fn confirm_with_always(prompt: &str) -> (bool, bool) {
    use std::io::{self, Write};
    
    loop {
        print!("{} [y/n/a]: ", prompt);
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return (true, false),
            "n" | "no" => return (false, false),
            "a" | "always" => return (true, true),
            _ => {
                println!("Please enter 'y' for yes, 'n' for no, or 'a' for always.");
            }
        }
    }
}
