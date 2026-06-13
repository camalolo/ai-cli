use std::future::Future;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::Local;
use futures::StreamExt;
use rig::agent::{AgentBuilder, HookAction, MultiTurnStreamItem, PromptHook, ToolCallHookAction};
use rig::completion::{CompletionModel, Message};
use rig::OneOrMany;
use rig::message::UserContent;
use rig::completion::AssistantContent;
use rig::providers::{ollama, openai, zai};
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig::client::{Nothing, CompletionClient};

use crate::config::Config;
use crate::tools::ToolCtx;

/// Maximum number of messages to keep in conversation history.
const MAX_HISTORY_SIZE: usize = 100;

/// High-level agent wrapper that builds rig agents on-the-fly per request.
pub struct AppAgent {
    config: Arc<Config>,
    preamble: String,
    history: Vec<Message>,
}

// ---------------------------------------------------------------------------
// CLI hook – prints streaming text and tool calls directly to stdout/stderr
// ---------------------------------------------------------------------------

/// Hook that prints LLM streaming events to stdout (text) and stderr (tool calls).
#[derive(Clone)]
struct CliHook;

impl<M> PromptHook<M> for CliHook
where
    M: CompletionModel,
{
    fn on_text_delta(
        &self,
        text_delta: &str,
        _aggregated_text: &str,
    ) -> impl Future<Output = HookAction> + Send {
        use std::io::Write;
        print!("{}", text_delta);
        let _ = std::io::stdout().flush();
        std::future::ready(HookAction::Continue)
    }

    fn on_tool_call(
        &self,
        tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        args: &str,
    ) -> impl Future<Output = ToolCallHookAction> + Send {
        let preview = crate::utils::truncate_str(args, 120);
        eprintln!("[→ {}] {}", tool_name, preview);
        std::future::ready(ToolCallHookAction::Continue)
    }

    fn on_tool_result(
        &self,
        tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        _args: &str,
        result: &str,
    ) -> impl Future<Output = HookAction> + Send {
        let display = crate::utils::truncate_str(result, 200);
        eprintln!("[✓ {}] {}", tool_name, display);
        std::future::ready(HookAction::Continue)
    }
}

// ---------------------------------------------------------------------------
// Macro to register all project tools onto an AgentBuilder
// ---------------------------------------------------------------------------

macro_rules! add_tools {
    ($builder:expr, $ctx:expr) => {{
        let ctx = $ctx;
        $builder
            .tool(crate::tools::command::CommandTool { ctx: ctx.clone() })
            .tool(crate::tools::search::SearchTool { ctx: ctx.clone() })
            .tool(crate::tools::scrape::ScrapeTool { ctx: ctx.clone() })
            .tool(crate::tools::email::EmailTool { ctx: ctx.clone() })
            .tool(crate::tools::alpha_vantage::AlphaVantageTool { ctx: ctx.clone() })
            .tool(crate::tools::file_editor::FileEditorTool { ctx })
    }};
}

// ---------------------------------------------------------------------------
// Helper: build provider clients
// ---------------------------------------------------------------------------

fn build_openai_client(config: &Config) -> Result<openai::CompletionsClient> {
    let client: openai::Client = if config.api_base_url.is_empty() {
        openai::Client::new(&config.api_key)
            .map_err(|e| anyhow!("Failed to create OpenAI client: {}", e))?
    } else {
        openai::Client::builder()
            .api_key(&config.api_key)
            .base_url(&config.api_base_url)
            .build()
            .map_err(|e| anyhow!("Failed to create OpenAI client: {}", e))?
    };
    Ok(client.completions_api())
}

fn build_zai_client(config: &Config) -> Result<zai::Client> {
    let mut builder = zai::Client::builder().api_key(&config.api_key);
    if config.api_base_url.is_empty() {
        builder = builder.coding();
    } else {
        builder = builder.base_url(&config.api_base_url);
    }
    builder
        .build()
        .map_err(|e| anyhow!("Failed to create ZAI client: {}", e))
}

fn build_ollama_client(config: &Config) -> Result<ollama::Client> {
    let client = if config.api_base_url.is_empty() {
        ollama::Client::new(Nothing)
            .map_err(|e| anyhow!("Failed to create Ollama client: {}", e))?
    } else {
        ollama::Client::builder()
            .api_key(config.api_key.clone())
            .base_url(&config.api_base_url)
            .build()
            .map_err(|e| anyhow!("Failed to create Ollama client: {}", e))?
    };
    Ok(client)
}

// ---------------------------------------------------------------------------
// AppAgent implementation
// ---------------------------------------------------------------------------

impl AppAgent {
    pub fn new(config: Config) -> Result<Self> {
        let preamble = Self::build_system_instruction();
        Ok(AppAgent {
            config: Arc::new(config),
            preamble,
            history: Vec::new(),
        })
    }

    pub fn model_name(&self) -> &str {
        &self.config.model
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    // -----------------------------------------------------------------------
    // System instruction
    // -----------------------------------------------------------------------

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
            "Today's date is {}. You are a proactive assistant running in a sandboxed {} terminal \
             environment (network access is disabled for commands) with a full set of command line \
             utilities. The default shell is {}. Your role is to assist with coding tasks, file \
             operations, online searches, email sending, and shell commands efficiently and \
             decisively. Assume the current directory (the sandbox root) is the target for all \
             commands. Take initiative to provide solutions, execute commands, and analyze results \
             immediately without asking for confirmation unless the action is explicitly ambiguous \
             (e.g., multiple repos) or potentially destructive (e.g., deleting files). Use the \
             `execute_command` tool to interact with the system but only when needed. Deliver \
             concise, clear responses. After running a command, always summarize its output \
             immediately and proceed with logical next steps, without waiting for the user to \
             prompt you further. Stay within the sandbox directory. Act confidently \
             and anticipate the user's needs to streamline their workflow. You may use md \
             formatting to provide a more readable response. When using search tools, prioritize \
             concise modes ('basic') to maintain efficiency unless the query requires depth.",
            today, os_name, shell_info
        )
    }

    // -----------------------------------------------------------------------
    // History management
    // -----------------------------------------------------------------------

    fn trim_history(&mut self) {
        if self.history.len() > MAX_HISTORY_SIZE {
            let excess = self.history.len() - MAX_HISTORY_SIZE;
            self.history.drain(..excess);
        }
    }

    // -----------------------------------------------------------------------
    // Streaming prompt – streams tokens to stdout, auto-approves tools
    // -----------------------------------------------------------------------

    /// Send a prompt with streaming output. Tokens are printed to stdout as they arrive.
    /// Tool calls are printed to stderr. Returns the full response text.
    /// Conversation history is passed to the model for multi-turn context.
    pub async fn stream_prompt(&mut self, msg: &str) -> Result<String> {
        let result = self.stream_prompt_inner(msg).await?;
        // Save to history after successful response
        self.history.push(Message::User {
            content: OneOrMany::one(UserContent::Text(msg.to_string().into())),
        });
        self.history.push(Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::Text(result.clone().into())),
        });
        self.trim_history();
        Ok(result)
    }

    async fn stream_prompt_inner(&self, msg: &str) -> Result<String> {
        let ctx = Arc::new(ToolCtx::new(self.config.clone(), false));
        let provider = self.config.provider.to_lowercase();
        let hook = CliHook;
        let history = &self.history;

        match provider.as_str() {
            "ollama" => {
                let client = build_ollama_client(&self.config)?;
                let model = client.completion_model(&self.config.model);
                let agent = add_tools!(
                    AgentBuilder::new(model)
                        .preamble(&self.preamble)
                        .default_max_turns(10)
                        .hook(hook),
                    ctx
                )
                .build();

                Self::run_stream(agent, msg, history).await
            }
            "zai" => {
                let client = build_zai_client(&self.config)?;
                let model = client.completion_model(&self.config.model);
                let agent = add_tools!(
                    AgentBuilder::new(model)
                        .preamble(&self.preamble)
                        .default_max_turns(10)
                        .hook(hook),
                    ctx
                )
                .build();

                Self::run_stream(agent, msg, history).await
            }
            _ => {
                let client = build_openai_client(&self.config)?;
                let model = client.completion_model(&self.config.model);
                let agent = add_tools!(
                    AgentBuilder::new(model)
                        .preamble(&self.preamble)
                        .default_max_turns(10)
                        .hook(hook),
                    ctx
                )
                .build();

                Self::run_stream(agent, msg, history).await
            }
        }
    }

    async fn run_stream<M>(
        agent: rig::agent::Agent<M, CliHook>,
        msg: &str,
        history: &[Message],
    ) -> Result<String>
    where
        M: CompletionModel + 'static,
    {
        let mut stream = agent
            .stream_prompt(msg)
            .with_history(history)
            .multi_turn(10)
            .await;

        let mut full_content = String::new();

        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text),
                )) => {
                    full_content.push_str(&text.text);
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ToolCall { .. },
                )) => {}
                Ok(MultiTurnStreamItem::StreamUserItem(_)) => {}
                Ok(MultiTurnStreamItem::FinalResponse(resp)) => {
                    full_content = resp.response().to_string();
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(anyhow!("Stream error: {}", e));
                }
            }
        }

        // Flush stdout after streaming
        use std::io::Write;
        let _ = std::io::stdout().flush();

        Ok(full_content)
    }
}
