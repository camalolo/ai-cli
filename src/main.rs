use clap::Parser;
use anyhow::Result;
use std::io::{self, Write, IsTerminal};
use build_time::build_time_local;

mod config;
use config::Config;

mod agent;
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
mod utils;

use agent::AppAgent;
use crate::utils::{log_to_file, clear_debug_file};

#[derive(Parser)]
#[command(name = "ai-cli")]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (", build_time_local!("%Y-%m-%d %H:%M:%S"), ")"))]
#[command(about = "A provider-agnostic AI assistant for coding tasks")]
struct Args {
    /// Single prompt to send to the LLM and exit
    #[arg(short, long)]
    prompt: Option<String>,

    /// Enable debug output for troubleshooting
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    clear_debug_file(args.debug);

    let config = Config::load()?;

    if args.debug {
        log_to_file(args.debug, "=== AI Provider Configuration ===");
        log_to_file(args.debug, &format!("Provider: {}", config.provider));
        log_to_file(args.debug, &format!("API Base URL: {}", config.api_base_url));
        log_to_file(args.debug, &format!("Model: {}", config.model));
        log_to_file(args.debug, &format!("API Key: {}***", &config.api_key.chars().take(4).collect::<String>()));
        log_to_file(args.debug, "================================");
    }

    let mut agent = AppAgent::new(config)?;

    if let Some(prompt) = args.prompt {
        // One-shot mode — stream response and exit
        match agent.stream_prompt(&prompt).await {
            Ok(_response) => {}
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Interactive REPL mode
    let is_tty = io::stdin().is_terminal();
    if is_tty {
        println!("ai-cli {} — model: {} — type /exit to quit",
            env!("CARGO_PKG_VERSION"),
            agent.model_name());
    }

    loop {
        if is_tty {
            print!("> ");
            io::stdout().flush()?;
        }

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => break, // EOF (Ctrl-D)
            Ok(_) => {}
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        }

        let line = input.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "/exit" | "/quit" | "/q" => break,
            "/clear" => {
                agent.clear_history();
                println!("History cleared.");
                continue;
            }
            "/model" => {
                println!("{}", agent.model_name());
                continue;
            }
            _ if line.starts_with('/') => {
                eprintln!("Unknown command: {} (try /exit, /clear, /model)", line);
                continue;
            }
            _ => {}
        }

        match agent.stream_prompt(line).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        if is_tty {
            println!(); // blank line between turns
        }
    }

    Ok(())
}
