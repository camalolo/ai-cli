use clap::Parser;
use anyhow::Result;
use colored::{Color, Colorize};
use std::sync::Arc;
use tokio::sync::Mutex;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use build_time::build_time_local;

mod config;
use config::Config;
use config::mask_value;

mod chat;
mod shell;
mod tools;
mod search;
mod scrape;

mod patch;
mod command;
mod email;
mod alpha_vantage;
mod file_edit;
mod sandbox;
mod http;

use crate::chat::ChatManager;
use crate::tools::{display_response, process_tool_calls};
use crate::shell::interactive_shell;
use crate::command::execute_command;
use sandbox::get_sandbox_root;

const COMPILE_TIME: &str = build_time_local!("%Y-%m-%d %H:%M:%S");

fn print_error(message: &str) {
    println!("{}", message.color(Color::Red));
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

fn log_to_file(debug: bool, msg: &str) {
    if debug {
        use std::fs::OpenOptions;
        use std::io::Write;
        use chrono::Utc;

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

async fn handle_llm_response(
    response: &serde_json::Value,
    chat_manager: Arc<Mutex<ChatManager>>,
    debug: bool,
    quiet: bool,
    allow_commands: bool,
    process_tools: bool,
) -> Result<()> {
    display_response(response);
    crate::tools::add_block_spacing();
    if process_tools {
        process_tool_calls(response, &chat_manager, debug, quiet, allow_commands).await?;
    }
    Ok(())
}

async fn send_llm_input(chat_manager: Arc<Mutex<ChatManager>>, llm_input: String, args: &Args) -> Result<()> {
    match chat_manager.lock().await.send_message(&llm_input, false, args.debug).await {
        Ok(response) => {
            handle_llm_response(&response, chat_manager.clone(), args.debug, false, false, false).await?;
        },
        Err(e) => print_error(&format!("Error: {}", e)),
    }
    Ok(())
}

async fn handle_user_input(
    user_input: &str,
    rl: &mut DefaultEditor,
    chat_manager: Arc<Mutex<ChatManager>>,
    args: &Args,
) -> Result<bool> {
    // Add to history (skip empty lines and special commands)
    let input_lower = user_input.to_lowercase();
    if !user_input.is_empty() && !input_lower.starts_with("exit") && !input_lower.starts_with("clear") {
        rl.add_history_entry(user_input).ok();
    }

    match input_lower.as_str() {
        "exit" => {
            println!("{}", "Goodbye!".color(Color::Cyan).bold());
            return Ok(false);
        }
        "clear" => {
            chat_manager.lock().await.create_chat();
            println!(
                "{}",
                "Conversation cleared! Starting fresh.".color(Color::Cyan)
            );
            println!();
            return Ok(true);
        }
        "" => {
            println!("{}", "Please enter a command or message.".color(Color::Red));
            println!();
            return Ok(true);
        }
        _ => {}
    }

    if let Some(command) = user_input.strip_prefix('!') {
        let command: &str = command.trim();
         if command.is_empty() {
            let output = interactive_shell(args.debug);
         let llm_input = format!("User ran interactive shell session with output:\n{}", output);
         send_llm_input(chat_manager.clone(), llm_input, args).await?;
         } else {
            let output = execute_command(command, args.debug).unwrap_or_else(|e| e.to_string());
            let llm_input = format!("User ran command '!{}' with output: {}", command, output);
            println!("{}", output);
            send_llm_input(chat_manager.clone(), llm_input, args).await?;
         }
     } else {
        let response = match chat_manager.lock().await.send_message(user_input, false, args.debug).await {
            Ok(resp) => resp,
            Err(e) => {
                println!(
                    "{}",
                    format!("Error: A generative AI error occurred: {}", e).color(Color::Red)
                );
                return Ok(true);
            }
        };

        println!(); // Add blank line before response
        if let Err(e) = handle_llm_response(&response, chat_manager.clone(), args.debug, false, false, true).await {
            print_error(&format!("Error processing tool calls: {}", e));
        }
    }
    Ok(true)
}

async fn load_and_display_config(debug: bool) -> Result<Config> {
    let config = Config::load()?;
    println!("Loaded config: base_url={}, version={}, model={}, key_present={}", config.api_base_url, config.api_version, config.model, !config.api_key.is_empty());

    if debug {
        log_to_file(debug, "=== AI Provider Configuration ===");
        log_to_file(debug, &format!("API Base URL: {}", config.api_base_url));
        log_to_file(debug, &format!("API Version: {}", config.api_version));
        log_to_file(debug, &format!("Model: {}", config.model));
        log_to_file(debug, &format!("API Key: {}***", &config.api_key.chars().take(4).collect::<String>()));
        log_to_file(debug, &format!("Endpoint: {}", config.get_api_endpoint()));
        log_to_file(debug, "Auth Method: Header (Bearer)");
        log_to_file(debug, "================================");
        log_to_file(debug, "=== SMTP Configuration ===");
        log_to_file(debug, &format!("SMTP_SERVER_IP: {}", config.smtp_server));
        log_to_file(debug, &format!("SMTP_USERNAME: {}", mask_value(&config.smtp_username, false)));
        log_to_file(debug, &format!("SMTP_PASSWORD: {}", mask_value(&config.smtp_password, true)));
        log_to_file(debug, &format!("DESTINATION_EMAIL: {}", mask_value(&config.destination_email, false)));
        log_to_file(debug, &format!("SENDER_EMAIL: {}", mask_value(&config.sender_email, false)));
        log_to_file(debug, "==========================");
    }
    Ok(config)
}

async fn handle_single_prompt_mode(chat_manager: Arc<Mutex<ChatManager>>, args: &Args) -> Result<()> {
    let prompt = args.prompt.as_ref().unwrap();
    println!("{}", "Processing single prompt...".color(Color::Cyan));
    let response = match chat_manager.lock().await.send_message(prompt, true, args.debug).await {
        Ok(resp) => {
            if args.debug {
                log_to_file(args.debug, "=== Raw Response ===");
                log_to_file(args.debug, &format!("{:?}", resp));
                log_to_file(args.debug, "===================");
            }
            resp
        },
        Err(e) => {
            print_error(&format!("Error: {}", e));
            chat_manager.lock().await.cleanup(false);
            return Err(e);
        }
    };
    if let Err(e) = handle_llm_response(&response, chat_manager.clone(), args.debug, true, args.allow_commands, true).await {
        print_error(&format!("Error processing tool calls: {}", e));
    }
    chat_manager.lock().await.cleanup(false);
    Ok(())
}

async fn run_interactive_loop(chat_manager: Arc<Mutex<ChatManager>>, args: &Args) -> Result<()> {
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
        format!("Working in sandbox: {}", *get_sandbox_root()).color(Color::Cyan)
    );
    println!(
        "{}",
        "Use !command to run shell commands directly (e.g., !ls or !dir). Use ! alone to enter interactive shell mode.".color(Color::Cyan)
    );
    println!();

    // Initialize rustyline editor
    let mut rl = DefaultEditor::new().expect("Failed to create readline editor");

    // Main input loop with rustyline
    loop {
        let conv_length: usize = chat_manager.lock().await
            .get_history()
            .iter()
            .filter_map(|msg| msg.get("content")?.as_str())
            .map(|s| s.len())
            .sum();

        let base_prompt = format!("[{}] > ", conv_length);
        let prompt = if cfg!(target_os = "windows") {
            // On Windows, avoid colored prompts due to compatibility issues
            base_prompt
        } else {
            base_prompt.color(Color::Green).bold().to_string()
        };

        let readline = rl.readline(&prompt);

        match readline {
            Ok(line) => {
                let user_input: &str = line.trim();

                match handle_user_input(user_input, &mut rl, chat_manager.clone(), args).await {
                    Ok(true) => continue,
                    Ok(false) => break,
                    Err(e) => {
                        print_error(&format!("Error: {}", e));
                        continue;
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Handle Ctrl-C
                println!("{}", "Interrupted".color(Color::Yellow));
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Handle Ctrl-D
                println!("{}", "Goodbye!".color(Color::Cyan).bold());
                break;
            }
            Err(err) => {
                print_error(&format!("Readline error: {}", err));
                continue;
            }
        }
    }

    chat_manager.lock().await.cleanup(false);

    Ok(())
}

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

    /// Allow LLM to execute commands without user confirmation in single prompt mode
    #[arg(long)]
    allow_commands: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let config = load_and_display_config(args.debug).await?;

    let chat_manager = Arc::new(Mutex::new(ChatManager::new(config)));

    if args.prompt.is_some() {
        handle_single_prompt_mode(chat_manager.clone(), &args).await?;
        return Ok(());
    }

    run_interactive_loop(chat_manager, &args).await?;

    Ok(())
}
