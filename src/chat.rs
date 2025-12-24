use chrono::Local;
use anyhow::{anyhow, Result};
use colored::{Color, Colorize};
use serde_json::{json, Value};
use crate::config::Config;
use spinners::{Spinner, Spinners};
use async_openai::{Client, config::OpenAIConfig, types::{CreateChatCompletionRequest, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent, ChatCompletionTool, ChatCompletionToolType, FunctionObject}};

#[derive(Debug)]
pub struct ChatManager {
    config: Config,
    history: Vec<Value>,
    system_instruction: String,
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
            "Today's date is {}. You are a proactive assistant running in a sandboxed {} terminal environment with a full set of command line utilities. The default shell is {}. Your role is to assist with coding tasks, file operations, online searches, email sending, and shell commands efficiently and decisively. Assume the current directory (the sandbox root) is the target for all commands. Take initiative to provide solutions, execute commands, and analyze results immediately without asking for confirmation unless the action is explicitly ambiguous (e.g., multiple repos) or potentially destructive (e.g., deleting files). Use the `execute_command` tool to interact with the system but only when needed. Deliver concise, clear responses. After running a command, always summarize its output immediately and proceed with logical next steps, without waiting for the user to prompt you further. Stay within the sandbox directory. Users can run shell commands directly with `!`, and you'll receive the output to assist further. Act confidently and anticipate the user's needs to streamline their workflow. You may use md formatting to provide a more readable response. When using search tools, prioritize concise modes ('basic') to maintain efficiency unless the query requires depth.",
            today, os_name, shell_info
        )
    }

    pub fn new(config: Config) -> Self {
        ChatManager {
            config,
            history: Vec::new(),
            system_instruction: Self::build_system_instruction(),
        }
    }

    pub fn create_chat(&mut self) {
        self.history.clear(); // Reset history, system_instruction persists
    }

    fn create_tool(name: &str, description: &str, parameters: serde_json::Value) -> ChatCompletionTool {
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: name.to_string(),
                description: Some(description.to_string()),
                parameters: Some(parameters),
                strict: Some(false),
            },
        }
    }

    fn build_tools() -> Vec<ChatCompletionTool> {
        vec![

            Self::create_tool("search_online", "Search the web for a query and return a synthesized answer. Use for factual lookups, current events, or research. Defaults to concise summaries for speed.", json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                     "include_results": {
                        "type": "boolean",
                        "description": "Whether to include a list of search results (default: false). Set to true only if you need to review sources directly, e.g., for verification or multiple options.",
                        "default": false
                    },
                    "answer_mode": {
                        "type": "string",
                        "enum": ["basic", "full"],
                        "description": "Answer detail level. 'basic' (default): Quick summary in 3 sentences, ideal for straightforward queries. 'full': Comprehensive answer with all available details, best for in-depth research or ambiguous topics.",
                        "default": "basic"
                    }
                },
                "required": ["query"]
            })),
            Self::create_tool("execute_command", "Execute a system command. Use this for any shell task.", json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            })),
            Self::create_tool("send_email", "Sends an email to a fixed address using SMTP.", json!({
                "type": "object",
                "properties": {
                    "subject": {"type": "string", "description": "Email subject line"},
                    "body": {"type": "string", "description": "Email message body"}
                },
                "required": ["subject", "body"]
            })),
            Self::create_tool("alpha_vantage_query", "Query the Alpha Vantage API for stock/financial data", json!({
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
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of most recent data points to return (default 5)",
                        "default": 5
                    }
                },
                "required": ["function", "symbol"]
            })),
            Self::create_tool("scrape_url", "Scrapes the content of a single URL", json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to scrape"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["summarized", "full"],
                        "default": "summarized",
                        "description": "Mode: 'summarized' provides a concise summary (default), 'full' returns complete extracted text"
                    }
                },
                "required": ["url"]
            })),
            Self::create_tool("file_editor", "Edit files in the sandbox with sub-commands: read, write, search, search_and_replace, apply_diff.", json!({
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
        ]
    }

    pub async fn send_message(&mut self, message: &str, skip_spinner: bool, debug: bool) -> Result<Value> {
        // Add user message to history in OpenAI format
        let user_message = json!({
            "role": "user",
            "content": message
        });
        self.history.push(user_message);

        crate::utils::log_to_file(debug, &format!("LLM Query: {}", crate::utils::truncate_str(message, 200)));



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
        let tools = Self::build_tools();

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

        crate::utils::log_to_file(debug, &format!("LLM Response: {}", crate::utils::truncate_str(&response_json.to_string(), 500)));

        // Add assistant response to history in OpenAI format
        for choice in &response.choices {
            self.history.push(serde_json::to_value(&choice.message)
                .map_err(|e| anyhow!("Failed to serialize message: {}", e))?);
        }

        Ok(response_json)
    }

    pub fn cleanup(&mut self, is_signal: bool) {
        self.history.clear();
        println!("{}", "Shutting down...".color(Color::Cyan));
        if is_signal {
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }
}