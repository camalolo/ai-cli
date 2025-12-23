use chrono::Local;
use anyhow::{anyhow, Result};
use colored::{Color, Colorize};
use serde_json::{json, Value};
use crate::config::Config;
use spinners::{Spinner, Spinners};
use async_openai::{Client, config::OpenAIConfig, types::{CreateChatCompletionRequest, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent, ChatCompletionTool, ChatCompletionToolType, FunctionObject}};



pub struct ChatManager {
    config: Config, // Store configuration
    history: Vec<Value>, // Stores user and assistant messages
    cleaned_up: bool,
    system_instruction: String, // Stored separately for the AI provider
}

impl ChatManager {
    pub fn get_tavily_api_key(&self) -> &str {
        &self.config.tavily_api_key
    }

    pub fn get_config(&self) -> &Config {
        &self.config
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

    pub async fn send_message(&mut self, message: &str, skip_spinner: bool) -> Result<Value> {
        // Add user message to history in OpenAI format
        let user_message = json!({
            "role": "user",
            "content": message
        });
        self.history.push(user_message);



        // Construct the body using async-openai types for type safety
        let mut chat_messages: Vec<ChatCompletionRequestMessage> = Vec::new();

        // Add system instruction
        chat_messages.push(ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: ChatCompletionRequestSystemMessageContent::Text(self.system_instruction.clone()),
            name: None,
        }));

        // Add conversation history
        for msg in &self.history {
            let message: ChatCompletionRequestMessage = serde_json::from_value(msg.clone())
                .map_err(|e| anyhow!("Failed to parse message: {}", e))?;
            chat_messages.push(message);
        }

        // Define tools using async-openai types
        let tools = vec![
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "search_online".to_string(),
                    description: Some("Searches the web for a given query. Use it to retrieve up to date information.".to_string()),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "The search query"
                            }
                        },
                        "required": ["query"]
                    })),
                    strict: Some(false),
                },
            },
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "execute_command".to_string(),
                    description: Some("Execute a system command. Use this for any shell task.".to_string()),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"}
                        },
                        "required": ["command"]
                    })),
                    strict: Some(false),
                },
            },
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "send_email".to_string(),
                    description: Some("Sends an email to a fixed address using SMTP.".to_string()),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "subject": {"type": "string", "description": "Email subject line"},
                            "body": {"type": "string", "description": "Email message body"}
                        },
                        "required": ["subject", "body"]
                    })),
                    strict: Some(false),
                },
            },
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "alpha_vantage_query".to_string(),
                    description: Some("Query the Alpha Vantage API for stock/financial data".to_string()),
                     parameters: Some(serde_json::json!({
                         "type": "object",
                         "properties": {
                             "function": {
                                 "type": "string",
                                 "description": "The Alpha Vantage function (e.g., TIME_SERIES_DAILY)"
                             },
                             "symbol": {
                                 "type": "string",
                                 "description": "The stock symbol (e.g., IBM)"
                             },
                             "outputsize": {
                                 "type": "string",
                                 "enum": ["compact", "full"],
                                 "description": "The size of the output data. 'compact' returns the last 100 data points, 'full' returns all available data. Defaults to 'compact'."
                             }
                         },
                         "required": ["function", "symbol"]
                    })),
                    strict: Some(false),
                },
            },
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "scrape_url".to_string(),
                    description: Some("Scrapes the content of a single URL".to_string()),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "url": {
                                "type": "string",
                                "description": "The URL to scrape"
                            }
                        },
                        "required": ["url"]
                    })),
                    strict: Some(false),
                },
            },
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: "file_editor".to_string(),
                    description: Some("Edit files in the sandbox with sub-commands: read, write, search, search_and_replace, apply_diff.".to_string()),
                    parameters: Some(serde_json::json!({
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
                    })),
                    strict: Some(false),
                },
            },
        ];

        let request = CreateChatCompletionRequest {
            model: self.config.model.clone(),
            messages: chat_messages,
            tools: Some(tools),
            ..Default::default()
        };

        let config = OpenAIConfig::new()
            .with_api_key(self.config.api_key.clone())
            .with_api_base(format!("{}/{}", self.config.api_base_url, self.config.api_version));
        let client = Client::with_config(config);

        let spinner = if skip_spinner {
            None
        } else {
            Some(Spinner::new(Spinners::Dots, "".into()))
        };

        let response = client.chat().create(request).await
            .map_err(|e| anyhow!("API request failed: {}", e))?;

        if let Some(mut spinner) = spinner {
            spinner.stop();
            print!("\r\x1b[2K");
        }

        let response_json: Value = serde_json::to_value(&response)
            .map_err(|e| anyhow!("Failed to serialize response: {}", e))?;

        // Add assistant response to history in OpenAI format
        for choice in &response.choices {
            self.history.push(serde_json::to_value(&choice.message)
                .map_err(|e| anyhow!("Failed to serialize message: {}", e))?);
        }

        Ok(response_json)
    }

    pub fn cleanup(&mut self, is_signal: bool) {
        if !self.cleaned_up {
            self.history.clear();
            self.cleaned_up = true;
            println!("{}", "Shutting down...".color(Color::Cyan));
            if is_signal {
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }
}