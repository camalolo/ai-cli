use chrono::Local;
use colored::{Color, Colorize};
use serde_json::{json, Value};
use reqwest::blocking::Client;
use std::time::Duration;
use crate::config::Config;
use crate::spinner::Spinner;

pub struct ChatManager {
    config: Config, // Store configuration
    history: Vec<Value>, // Stores user and assistant messages
    cleaned_up: bool,
    system_instruction: String, // Stored separately for the AI provider
}

impl ChatManager {
    pub fn get_google_search_api_key(&self) -> &str {
        &self.config.google_search_api_key
    }

    pub fn get_google_search_engine_id(&self) -> &str {
        &self.config.google_search_engine_id
    }

    pub fn get_smtp_server(&self) -> &str {
        &self.config.smtp_server
    }

    pub fn get_alpha_vantage_api_key(&self) -> &str {
        &self.config.alpha_vantage_api_key
    }

    pub fn get_history(&self) -> &Vec<serde_json::Value> {
        &self.history
    }

    fn build_system_instruction() -> String {
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

        let shell_info = crate::shell::detect_shell_info();

        format!(
            "Today's date is {}. You are a proactive assistant running in a sandboxed {} terminal environment with a full set of command line utilities. The default shell is {}. Your role is to assist with coding tasks, file operations, online searches, email sending, and shell commands efficiently and decisively. Assume the current directory (the sandbox root) is the target for all commands. Take initiative to provide solutions, execute commands, and analyze results immediately without asking for confirmation unless the action is explicitly ambiguous (e.g., multiple repos) or potentially destructive (e.g., deleting files). Use the `execute_command` tool to interact with the system but only when needed. Deliver concise, clear responses. After running a command, always summarize its output immediately and proceed with logical next steps, without waiting for the user to prompt you further. When reading files or executing commands, summarize the results intelligently for the user without dumping raw output unless explicitly requested. Stay within the sandbox directory. Users can run shell commands directly with `!`, and you'll receive the output to assist further. Act confidently and anticipate the user's needs to streamline their workflow. You may use md formatting to provide a more readable response.",
            today, os_name, shell_info
        )
    }

    pub fn new(config: Config) -> Self {
        ChatManager {
            config,
            history: Vec::new(), // Start empty; system_instruction is separate
            cleaned_up: false,
            system_instruction: Self::build_system_instruction(),
        }
    }

    pub fn create_chat(&mut self) {
        self.history.clear(); // Reset history, system_instruction persists
    }

    pub fn send_message(&mut self, message: &str, skip_spinner: bool) -> Result<Value, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

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

        let mut spinner = if skip_spinner { None } else { Some(Spinner::new()) };
        if let Some(ref mut spinner) = spinner {
            spinner.start();
        }

        // Build request with configurable endpoint and authentication
        let endpoint = self.config.get_api_endpoint();
        let mut request = client.post(&endpoint);

        // Add authentication based on API type
        if let Some(auth_header) = self.config.get_auth_header() {
            request = request.header("Authorization", auth_header);
        }

        let response = request
            .json(&body)
            .send()
            .map_err(|e| format!("API request failed: {}", e))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Authentication failed. Please check your API key in ~/.aicli.conf".to_string());
        }

        if let Some(ref mut spinner) = spinner {
            spinner.stop();
        }

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

    pub fn cleanup(&mut self, is_signal: bool) {
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