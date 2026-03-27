use chrono::Utc;
use colored::{Color, Colorize};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;

pub fn print_error(message: &str) {
    println!("{}", message.color(Color::Red));
}

pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        format!("{}...", s.chars().take(max_len).collect::<String>())
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
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
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

/// Summarizes text to approximately `num_sentences` sentences using extractive summarization.
pub fn summarize_text(text: &str, num_sentences: usize) -> String {
    let mut summariser = pithy::Summariser::new();
    summariser.add_raw_text("content".to_string(), text.to_string(), ".", 10, 500, false);
    let top_sentences = summariser.approximate_top_sentences(num_sentences, 0.3, 0.1);
    top_sentences
        .into_iter()
        .map(|s| s.text)
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let result = truncate_str("hello world", 5);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_unicode() {
        let result = truncate_str("🔥🔥🔥🔥🔥🔥🔥🔥🔥🔥", 5);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_get_opt_str_existing() {
        let args = json!({"key": "value"});
        assert_eq!(get_opt_str(&args, "key", "default"), "value");
    }

    #[test]
    fn test_get_opt_str_missing() {
        let args = json!({"other": "value"});
        assert_eq!(get_opt_str(&args, "key", "default"), "default");
    }

    #[test]
    fn test_get_opt_str_wrong_type() {
        let args = json!({"key": 42});
        assert_eq!(get_opt_str(&args, "key", "default"), "default");
    }

    #[test]
    fn test_get_opt_bool_true() {
        let args = json!({"key": true});
        assert!(get_opt_bool(&args, "key", false));
    }

    #[test]
    fn test_get_opt_bool_false() {
        let args = json!({"key": false});
        assert!(!get_opt_bool(&args, "key", true));
    }

    #[test]
    fn test_get_opt_bool_missing() {
        let args = json!({});
        assert_eq!(get_opt_bool(&args, "key", true), true);
    }

    #[test]
    fn test_summarize_text_short() {
        let text = "This is a short sentence.";
        let result = summarize_text(text, 1);
        assert!(!result.is_empty());
        assert!(result.contains("short"));
    }

    #[test]
    fn test_summarize_text_multiple_sentences() {
        let text = "First sentence here. Second sentence about cats. Third sentence about dogs. Fourth sentence about birds.";
        let result = summarize_text(text, 2);
        let sentence_count = result.matches('.').count();
        assert!(sentence_count <= 3);
    }
}
