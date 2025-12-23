use clap::Parser;
use anyhow::Result;
use colored::{Color, Colorize};
use std::sync::{Arc, Mutex};
use build_time::build_time_local;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

mod config;
use config::Config;

mod chat;
mod shell;
mod tools;
mod search;
mod scrape;
mod similarity;
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

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    // Load configuration
    let config = Config::load()?;
    println!("Loaded config: base_url={}, version={}, model={}, key_present={}", config.api_base_url, config.api_version, config.model, !config.api_key.is_empty());

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
        let response = match chat_manager.lock().unwrap().send_message(&prompt, true) {
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
                chat_manager.lock().unwrap().cleanup(false);
                return Err(e);
            }
        };
        display_response(&response);
        crate::tools::add_block_spacing();
        if let Err(e) = process_tool_calls(&response, &chat_manager, args.debug, true) {
            println!("{}", format!("Error processing tool calls: {}", e).color(Color::Red));
        }
        chat_manager.lock().unwrap().cleanup(false);
        return Ok(());
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
        let conv_length: usize = chat_manager.lock().map(|manager| {
            manager
                .get_history()
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

        let readline = rl.readline(&prompt);

        match readline {
            Ok(line) => {
                let user_input: &str = line.trim();

                // Add to history (skip empty lines and special commands)
                if !user_input.is_empty() && !user_input.to_lowercase().starts_with("exit") && !user_input.to_lowercase().starts_with("clear") {
                    rl.add_history_entry(user_input).ok();
                }

                match user_input.to_lowercase().as_str() {
                    "exit" => {
                        println!("{}", "Goodbye!".color(Color::Cyan).bold());
                        break;
                    }
                    "clear" => {
                        chat_manager.lock().unwrap().create_chat();
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

                if let Some(command) = user_input.strip_prefix('!') {
                    let command: &str = command.trim();
                    if command.is_empty() {
                        let output = interactive_shell();
                        let llm_input = format!("User ran interactive shell session with output:\n{}", output);
                        match chat_manager.lock() {
                            Ok(mut mgr) => match mgr.send_message(&llm_input, false) {
                                Ok(response) => {
                                    display_response(&response);
                                    crate::tools::add_block_spacing();
                                },
                                Err(e) => println!("{}", format!("Error: {}", e).color(Color::Red)),
                            },
                            Err(e) => println!("{}", format!("Lock error: {}", e).color(Color::Red)),
                        }
                    } else {
                        let output = execute_command(command);
                        println!();
                        let llm_input = format!("User ran command '!{}' with output: {}", command, output);
                        match chat_manager.lock() {
                            Ok(mut mgr) => match mgr.send_message(&llm_input, false) {
                                Ok(response) => {
                                    display_response(&response);
                                    crate::tools::add_block_spacing();
                                },
                                Err(e) => println!("{}", format!("Error: {}", e).color(Color::Red)),
                            },
                            Err(e) => println!("{}", format!("Lock error: {}", e).color(Color::Red)),
                        }
                    }
                    } else {
                        let response = match chat_manager.lock().unwrap().send_message(user_input, false) {
                            Ok(resp) => resp,
                            Err(e) => {
                                println!(
                                    "{}",
                                    format!("Error: A generative AI error occurred: {}", e).color(Color::Red)
                                );
                                continue;
                            }
                        };

                        println!(); // Add blank line before response
                        display_response(&response);
                    crate::tools::add_block_spacing();

                    if let Err(e) = process_tool_calls(&response, &chat_manager, args.debug, false) {
                        println!("{}", format!("Error processing tool calls: {}", e).color(Color::Red));
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
                println!("{}", format!("Readline error: {}", err).color(Color::Red));
                continue;
            }
        }
    }

    chat_manager.lock().unwrap().cleanup(false);

    Ok(())
}
