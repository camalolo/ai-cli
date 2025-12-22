use build_time::build_time_local;
use chrono::Local;
use clap::Parser;
use colored::{Color, Colorize};
use ctrlc;
#[allow(unused_imports)]
use dotenv::from_path;

use reqwest::blocking::Client;
use std::io::{self, Write};
use serde_json::{json, Value};
use std::env;

use std::sync::{Arc, Mutex};

// Declare the config module
mod config;
use config::Config;

#[derive(Parser)]
#[command(name = "ai-cli")]
#[command(about = "A provider-agnostic AI assistant for coding tasks")]
struct Args {
    /// Single prompt to send to the LLM and exit
    #[arg(short, long)]
    prompt: Option<String>,

    /// Enable debug output for troubleshooting
    #[arg(long)]
    debug: bool,
}

// Declare and import the search module
mod search;
#[allow(unused_imports)]
use search::{scrape_url, search_online};

mod command;
mod email;
mod alpha_vantage;
mod file_edit;
mod spinner;
mod sandbox;
mod http; // Spinner module

use command::execute_command;
use email::send_email;
use alpha_vantage::alpha_vantage_query;
use file_edit::file_editor;
use crate::spinner::Spinner; // Import the Spinner
use sandbox::SANDBOX_ROOT;

const COMPILE_TIME: &str = build_time_local!("%Y-%m-%d %H:%M:%S");

fn process_execute_command(args: &Value) -> String {
    let command = args.get("command").and_then(|c| c.as_str());
    if let Some(cmd) = command {
        println!("LLM wants to execute command: {} | Confirm execution? (y/n)", cmd.color(Color::Magenta));
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read input");
        let input = input.trim().to_lowercase();
        if input == "y" || input == "yes" {
            println!("Executing command: {}", cmd.color(Color::Magenta));
            let result = execute_command(cmd);
            format!("[Tool result] execute_command: {}", result)
        } else {
            "[Tool result] execute_command: User rejected the command execution.".to_string()
        }
    } else {
        "[Tool error] execute_command: Missing 'command' parameter".to_string()
    }
}

fn process_search_online(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>) -> String {
    let query = args.get("query").and_then(|q| q.as_str());
    if let Some(q) = query {
        let api_key_result = chat_manager.lock().map(|manager| manager.config.google_search_api_key.clone());
        let engine_id_result = chat_manager.lock().map(|manager| manager.config.google_search_engine_id.clone());
        match (api_key_result, engine_id_result) {
            (Ok(api_key), Ok(engine_id)) => {
                let result = search_online(q, &api_key, &engine_id);
                format!("[Tool result] search_online: {}", result)
            }
            _ => "[Tool error] search_online: Failed to access configuration".to_string(),
        }
    } else {
        "[Tool error] search_online: Missing 'query' parameter".to_string()
    }
}

fn process_send_email(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> String {
    let subject = args.get("subject").and_then(|s| s.as_str());
    let body = args.get("body").and_then(|b| b.as_str());

    if let (Some(subj), Some(bod)) = (subject, body) {
        let smtp_server_result = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e)).map(|manager| manager.config.smtp_server.clone());
        match smtp_server_result {
            Ok(smtp_server) => match send_email(subj, bod, &smtp_server, debug) {
                Ok(msg) => format!("[Tool result] send_email: {}", msg),
                Err(e) => format!("[Tool error] send_email: {}", e),
            },
            Err(e) => format!("[Tool error] send_email: {}", e),
        }
    } else {
        "[Tool error] send_email: Missing required parameters".to_string()
    }
}



fn detect_shell_info() -> String {
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
            if let Ok(version_output) = std::process::Command::new("bash")
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
            if let Ok(version_output) = std::process::Command::new("bash")
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
            if let Ok(version_output) = std::process::Command::new("powershell")
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
            if let Ok(version_output) = std::process::Command::new(cmd)
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

struct ChatManager {
    config: Config, // Store configuration
    history: Vec<Value>, // Stores user and assistant messages
    cleaned_up: bool,
    system_instruction: String, // Stored separately for Gemini
}

impl ChatManager {
    fn new(config: Config) -> Self {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let os_name = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "linux") {
            "Linux"
        } else {
            "Unix-like"
        };

        let shell_info = detect_shell_info();

        let system_instruction = format!(
            "Today's date is {}. You are a proactive assistant running in a sandboxed {} terminal environment with a full set of command line utilities. The default shell is {}. Your role is to assist with coding tasks, file operations, online searches, email sending, and shell commands efficiently and decisively. Assume the current directory (the sandbox root) is the target for all commands. Take initiative to provide solutions, execute commands, and analyze results immediately without asking for confirmation unless the action is explicitly ambiguous (e.g., multiple repos) or potentially destructive (e.g., deleting files). Use the `execute_command` tool to interact with the system but only when needed. Deliver concise, clear responses. After running a command, always summarize its output immediately and proceed with logical next steps, without waiting for the user to prompt you further. When reading files or executing commands, summarize the results intelligently for the user without dumping raw output unless explicitly requested. Stay within the sandbox directory. Users can run shell commands directly with `!`, and you'll receive the output to assist further. Act confidently and anticipate the user's needs to streamline their workflow.",
            today, os_name, shell_info
        );
        ChatManager {
            config,
            history: Vec::new(), // Start empty; system_instruction is separate
            cleaned_up: false,
            system_instruction,
        }
    }

    fn create_chat(&mut self) {
        self.history.clear(); // Reset history, system_instruction persists
    }

    fn send_message(&mut self, message: &str) -> Result<Value, String> {
        let client = Client::new();

        // Add user message to history in OpenAI format
        let user_message = json!({
            "role": "user",
            "content": message
        });
        self.history.push(user_message);

        // Construct messages array with system instruction and history
        let mut messages = Vec::new();
        
        // Add system instruction as first message
        messages.push(json!({
            "role": "system",
            "content": &self.system_instruction
        }));
        
        // Add conversation history
        messages.extend_from_slice(&self.history);

        // Construct the body in OpenAI-compatible format
        let body = json!({
            "model": self.config.model,
            "messages": messages,
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "search_online",
                        "description": "Searches the web for a given query. Use it to retrieve up to date information.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "The search query",
                                }
                            },
                            "required": ["query"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "execute_command",
                        "description": "Execute a system command. Use this for any shell task.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "command": {"type": "string"}
                            },
                            "required": ["command"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "send_email",
                        "description": "Sends an email to a fixed address using SMTP.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "subject": {"type": "string", "description": "Email subject line"},
                                "body": {"type": "string", "description": "Email message body"}
                            },
                            "required": ["subject", "body"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "alpha_vantage_query",
                        "description": "Query the Alpha Vantage API for stock/financial data",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "function": {
                                    "type": "string",
                                    "description": "The Alpha Vantage function (e.g., TIME_SERIES_DAILY)"
                                },
                                "symbol": {
                                    "type": "string",
                                    "description": "The stock symbol (e.g., IBM)"
                                }
                            },
                            "required": ["function", "symbol"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "scrape_url",
                        "description": "Scrapes the content of a single URL",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "url": {
                                    "type": "string",
                                    "description": "The URL to scrape",
                                }
                            },
                            "required": ["url"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "file_editor",
                        "description": "Edit files in the sandbox with sub-commands: read, write, search, search_and_replace, apply_diff.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "subcommand": {
                                    "type": "string",
                                    "description": "The sub-command to execute: read, write, search, search_and_replace, apply_diff",
                                    "enum": ["read", "write", "search", "search_and_replace", "apply_diff"]
                                },
                                "filename": {
                                    "type": "string",
                                    "description": "The name of the file in the sandbox to operate on"
                                },
                                "data": {
                                    "type": "string",
                                    "description": "Content to write (for write), regex pattern (for search/search_and_replace), or diff content (for apply_diff)"
                                },
                                "replacement": {
                                    "type": "string",
                                    "description": "Replacement text for search_and_replace"
                                }
                            },
                            "required": ["subcommand", "filename"]
                        }
                    }
                }
            ]
        });

        let mut spinner = Spinner::new();
        spinner.start();

        // Build request with configurable endpoint and authentication
        let endpoint = self.config.get_api_endpoint();
        let mut request = client.post(&endpoint);
        
        // Add authentication based on API type
        if let Some(auth_header) = self.config.get_auth_header() {
            request = request.header("Authorization", auth_header);
        }
        if let Some((key, value)) = self.config.get_auth_query() {
            request = request.query(&[(key, value)]);
        }
        
        let response = request
            .json(&body)
            .send()
            .map_err(|e| format!("API request failed: {}", e))?;

        spinner.stop();

        let response_json: Value = response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Add assistant response to history in OpenAI format
        if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                if let Some(message) = choice.get("message") {
                    self.history.push(message.clone());
                }
            }
        }

        Ok(response_json)
    }

    fn cleanup(&mut self, is_signal: bool) {
        if !self.cleaned_up {
            self.history.clear();
            self.cleaned_up = true;
            println!("{}", "Shutting down...".color(Color::Cyan));
            std::thread::sleep(std::time::Duration::from_secs(if is_signal {
                3
            } else {
                2
            }));
        }
    }
}

fn display_response(response: &Value) {
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("{}", content.color(Color::Yellow));
                }
            }
        }
    }
    println!(); // Add a newline after the response
}

fn process_tool_calls(response: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> Result<(), String> {
    let mut current_response = response.clone();

    loop {
        let tool_calls: Vec<(String, Value)> = current_response
            .get("choices")
            .and_then(|c| c.as_array())
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|choice| {
                choice
                    .get("message")
                    .and_then(|m| m.get("tool_calls"))
                    .and_then(|tc| tc.as_array())
                    .map(|tool_calls| {
                        tool_calls
                            .iter()
                            .filter_map(|tc| {
                                let func = tc.get("function");
                                let name = func
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let args = func
                                    .and_then(|f| f.get("arguments"))
                                    .and_then(|a| serde_json::from_str::<Value>(a.as_str()?).ok())
                                    .unwrap_or(json!({}));
                                if !name.is_empty() {
                                    Some((name, args))
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();

        if tool_calls.is_empty() {
            break;
        }

        let mut results = Vec::new();
        for (func_name, args) in tool_calls {
            match func_name.as_str() {
                "execute_command" => {
                    let result = process_execute_command(&args);
                    results.push(result);
                }
                "search_online" => {
                    let result = process_search_online(&args, chat_manager);
                    results.push(result);
                }
                "scrape_url" => {
                    let url = args.get("url").and_then(|u| u.as_str());
                    if let Some(u) = url {
                        let result = search::scrape_url(u);
                        if result.starts_with("Error") || result.starts_with("Skipped") {
                            println!("Scrape failed: {}", result);
                        }
                        results.push(format!("[Tool result] scrape_url: {}", result));
                    } else {
                        results.push(
                            "[Tool error] scrape_url: Missing 'url' parameter".to_string(),
                        );
                    }
                }
                "send_email" => {
                    let result = process_send_email(&args, chat_manager, debug);
                    results.push(result);
                }
                "alpha_vantage_query" => {
                    let function = args.get("function").and_then(|f| f.as_str());
                    let symbol = args.get("symbol").and_then(|s| s.as_str());
                    if let (Some(func), Some(sym)) = (function, symbol) {
                        let api_key = {
                            let manager = chat_manager.lock().unwrap();
                            manager.config.alpha_vantage_api_key.clone()
                        };
                        match alpha_vantage_query(func, sym, &api_key) {
                            Ok(result) => results.push(format!(
                                "[Tool result] alpha_vantage_query: {}",
                                result
                            )),
                            Err(e) => results
                                .push(format!("[Tool error] alpha_vantage_query: {}", e)),
                        }
                    } else {
                        results.push(
                            "[Tool error] alpha_vantage_query: Missing required parameters"
                                .to_string(),
                        );
                    }
                }
                "file_editor" => {
                    let subcommand = args.get("subcommand").and_then(|s| s.as_str());
                    let filename = args.get("filename").and_then(|f| f.as_str());
                    let data = args.get("data").and_then(|d| d.as_str());
                    let replacement = args.get("replacement").and_then(|r| r.as_str());

                    if let (Some(subcmd), Some(fname)) = (subcommand, filename) {
                        let skip_confirmation = matches!(subcmd, "read" | "search"); // Only skip for non-destructive ops
                        let result = file_editor(subcmd, fname, data, replacement, skip_confirmation);
                        results.push(format!("[Tool result] file_editor: {}", result));
                    } else {
                        results.push("[Tool error] file_editor: Missing required parameters 'subcommand' or 'filename'".to_string());
                    }
                }
                _ => {
                    results.push(format!("[Tool error] Unknown function: {}", func_name));
                }
            }
        }

        if !results.is_empty() {
            let combined_results = results.join("\n");
            current_response = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e))?.send_message(&combined_results)?;
            display_response(&current_response);
        } else {
            break;
        }
    }

    Ok(())
}

fn interactive_shell() -> String {
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

fn main() {
    let args = Args::parse();

    // Load configuration
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("{}", format!("Error: {}", e).color(Color::Red));
            std::process::exit(1);
        }
    };

    // Debug output for configuration
    if args.debug {
        config.display_summary();
        println!("{}", "=== SMTP Configuration ===".color(Color::Cyan));
        println!("SMTP_SERVER_IP: {}", config.smtp_server);

        let smtp_username = if config.smtp_username.is_empty() {
            "<not set>".to_string()
        } else {
            config.smtp_username.clone()
        };
        let smtp_password = if config.smtp_password.is_empty() {
            "<not set>".to_string()
        } else {
            "***masked***".to_string()
        };
        println!("SMTP_USERNAME: {}", smtp_username);
        println!("SMTP_PASSWORD: {}", smtp_password);

        let destination_email = if config.destination_email.is_empty() {
            "<not set>".to_string()
        } else {
            config.destination_email.clone()
        };
        let sender_email = if config.sender_email.is_empty() {
            "<not set>".to_string()
        } else {
            config.sender_email.clone()
        };
        println!("DESTINATION_EMAIL: {}", destination_email);
        println!("SENDER_EMAIL: {}", sender_email);
        println!("{}", "==========================".color(Color::Cyan));
        println!();
    }

    let chat_manager = Arc::new(Mutex::new(ChatManager::new(config)));
    let chat_manager_clone = Arc::clone(&chat_manager);

    ctrlc::set_handler(move || {
        let mut manager = chat_manager_clone.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire chat manager lock: {}", e);
            std::process::exit(1);
        });
        manager.cleanup(true);
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // Handle single prompt mode
    if let Some(prompt) = args.prompt {
        println!("{}", "Processing single prompt...".color(Color::Cyan));
        let response = match chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e)).and_then(|mut mgr| mgr.send_message(&prompt)) {
            Ok(resp) => {
                if args.debug {
                    println!("{}", "=== Raw Response ===".color(Color::Cyan));
                    println!("{:?}", resp);
                    println!("{}", "===================".color(Color::Cyan));
                }
                resp
            },
            Err(e) => {
                println!("{}", format!("Error: {}", e).color(Color::Red));
                let _ = chat_manager.lock().map(|mut mgr| mgr.cleanup(false));
                std::process::exit(1);
            }
        };
        display_response(&response);
        if let Err(e) = process_tool_calls(&response, &chat_manager, args.debug) {
            println!("{}", format!("Error processing tool calls: {}", e).color(Color::Red));
        }
        let _ = chat_manager.lock().map(|mut mgr| mgr.cleanup(false));
        return;
    }

    println!(
        "{}",
        "Welcome to AI CLI! Chat with me (type 'exit' to quit, 'clear' to reset conversation)."
            .color(Color::Cyan)
            .bold()
    );
    println!(
        "{}",
        format!("Version: {}", COMPILE_TIME).color(Color::Cyan)
    );
    println!(
        "{}",
        format!("Working in sandbox: {}", *SANDBOX_ROOT).color(Color::Cyan)
    );
    println!(
        "{}",
        "Use !command to run shell commands directly (e.g., !ls or !dir). Use ! alone to enter interactive shell mode.".color(Color::Cyan)
    );
    println!();

    // Simple input handling for better Windows compatibility
    loop {
        let conv_length: usize = chat_manager.lock().map(|manager| {
            manager
                .history
                .iter()
                .filter_map(|msg| {
                    msg.get("content").and_then(|c| c.as_str()).map(|s| s.len())
                })
                .sum()
        }).unwrap_or_else(|e| {
            println!("Failed to acquire chat manager lock: {}", e);
            0
        });

        let prompt = {
            #[cfg(target_os = "windows")]
            {
                // On Windows, avoid colored prompts due to compatibility issues
                format!("[{}] > ", conv_length)
            }
            #[cfg(not(target_os = "windows"))]
            {
                format!("[{}] > ", conv_length).color(Color::Green).bold().to_string()
            }
        };

        print!("{}", prompt);
        io::stdout().flush().expect("Failed to flush stdout");

        let mut user_input = String::new();
        match io::stdin().read_line(&mut user_input) {
            Ok(_) => {
                let user_input = user_input.trim();

                match user_input.to_lowercase().as_str() {
                    "exit" => {
                        println!("{}", "Goodbye!".color(Color::Cyan).bold());
                        break;
                    }
                    "clear" => {
                        let _ = chat_manager.lock().map(|mut mgr| mgr.create_chat());
                        println!(
                            "{}",
                            "Conversation cleared! Starting fresh.".color(Color::Cyan)
                        );
                        println!();
                        continue;
                    }
                    "" => {
                        println!("{}", "Please enter a command or message.".color(Color::Red));
                        println!();
                        continue;
                    }
                    _ => {}
                }

                if user_input.starts_with('!') {
                    let command = user_input[1..].trim();
                    if command.is_empty() {
                        let output = interactive_shell();
                        let llm_input = format!("User ran interactive shell session with output:\n{}", output);
                        match chat_manager.lock() {
                            Ok(mut mgr) => match mgr.send_message(&llm_input) {
                                Ok(response) => display_response(&response),
                                Err(e) => println!("{}", format!("Error: {}", e).color(Color::Red)),
                            },
                            Err(e) => println!("{}", format!("Lock error: {}", e).color(Color::Red)),
                        }
                    } else {
                        let output = execute_command(command);
                        println!(
                            "{}",
                            format!("Command output: {}", output).color(Color::Magenta)
                        );
                        let llm_input = format!("User ran command '!{}' with output: {}", command, output);
                        match chat_manager.lock() {
                            Ok(mut mgr) => match mgr.send_message(&llm_input) {
                                Ok(response) => display_response(&response),
                                Err(e) => println!("{}", format!("Error: {}", e).color(Color::Red)),
                            },
                            Err(e) => println!("{}", format!("Lock error: {}", e).color(Color::Red)),
                        }
                    }
                } else {
                    let response = match chat_manager.lock().unwrap().send_message(user_input) {
                        Ok(resp) => resp,
                        Err(e) => {
                            println!(
                                "{}",
                                format!("Error: A generative AI error occurred: {}", e).color(Color::Red)
                            );
                            continue;
                        }
                    };

                    display_response(&response);

                    if let Err(e) = process_tool_calls(&response, &chat_manager, args.debug) {
                        println!("{}", format!("Error processing tool calls: {}", e).color(Color::Red));
                    }
                }
            }
            Err(e) => {
                println!("{}", format!("Input error: {}", e).color(Color::Red));
                continue;
            }
        }
    }

    let _ = chat_manager.lock().map(|mut mgr| mgr.cleanup(false));
}
