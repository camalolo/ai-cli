# AGENTS.md — ai-cli

## Overview

ai-cli is a Rust TUI application that acts as a provider-agnostic AI coding assistant. It uses the `rig` crate to manage LLM interactions with multi-turn tool calling, renders a terminal UI with `ratatui`, and executes commands inside a sandboxed environment (bubblewrap on Linux). The binary is `ai-cli`.

## Build & Run

```bash
cargo build                # debug build
cargo build --release      # optimized (LTO fat, codegen-units 1, stripped)
cargo run                  # TUI mode (default) — requires ~/.aicli.conf
cargo run -- --prompt "msg"  # one-shot mode, no TUI
cargo run -- --debug       # enables debug.log in cwd
cargo run -- --quiet       # suppress reasoning, show only final result
cargo test                 # run all unit tests (37 tests)
```

No Makefile, CI config, or linting setup exists. `.cargo/config.toml` adds `-C target-cpu=native`.

## Architecture

```
main.rs ── parses args, loads Config, creates AppAgent
  ├── agent.rs ── AppAgent: builds rig agents per-request, manages history
  │     ├── add_tools! / add_tools_with_hook! macros register all tools
  │     ├── TuiHook (PromptHook impl) bridges streaming events to mpsc channel
  │     └── build_*_client() selects provider (openai-compatible, ollama, zai)
  ├── tui/ ── Elm architecture (init → loop { view → event → update })
  │     ├── mod.rs    ── run_tui(): event loop
  │     ├── model.rs  ── App struct (all UI state), ChatMessage, AppState
  │     ├── update.rs ── handle_event(): dispatches AppEvent mutations
  │     ├── view.rs   ── render(): draws chat, input, status bar, sidebar
  │     ├── theme.rs  ── dark theme colors, spinner frames
  │     └── event.rs  ── spawns crossterm key reader + ~12Hz tick timer
  ├── event.rs ── AppEvent enum (LlmToken, LlmDone, ToolCall, NeedConfirmation, etc.)
  ├── tools/ ── rig Tool implementations, all share ToolCtx
  │     ├── mod.rs        ── ToolCtx (config + confirm/notify callbacks), ToolRunError
  │     ├── command.rs    ── execute_command tool
  │     ├── file_editor.rs─ file_editor tool (read/write/search/replace/apply_diff)
  │     ├── search.rs     ── Tavily web search tool
  │     ├── scrape.rs     ── URL content scraper tool
  │     ├── email.rs      ── SMTP email tool
  │     └── alpha_vantage.rs ── financial data tool
  ├── config.rs ── loads ~/.aicli.conf (INI format) + AICLI_ env vars
  ├── command.rs ── bwrap sandbox on Linux, direct exec on other OSes
  ├── file_edit.rs ── path resolution (sandbox-bounded), file operations
  ├── sandbox.rs ── SANDBOX_ROOT (OnceLock of cwd)
  ├── patch.rs ── unified diff application via patch-apply crate
  └── utils.rs ── debug logging, truncation, text summarization
```

### Control Flow

1. **One-shot** (`--prompt`): `AppAgent::single_prompt()` → rig agent with auto-approve `ToolCtx` → print response.
2. **TUI** (default): Elm loop in `tui/mod.rs`. User input spawns `start_tui_stream()` on a tokio task, which holds the `Mutex<AppAgent>` lock for the entire stream. Events (`AppEvent`) flow through an `mpsc::unbounded_channel`. The `TuiHook` intercepts tool calls for user confirmation via oneshot channels.

### Key Data Flows

- **History**: `AppAgent.history` holds `Vec<serde_json::Value>` messages. `trim_history()` ensures it stays under 100 messages, cutting at message boundaries (never mid-tool-call).
- **Confirmation**: Two separate paths — `TuiHook.on_tool_call()` for rig-managed tool calls, `ToolCtx.confirm` for tool-internal confirmations (file writes). Both resolve through `AppEvent::NeedConfirmation` + oneshot channel.
- **Sandbox**: `sandbox::SANDBOX_ROOT` is the cwd at startup. All file operations go through `file_edit::resolve_sandbox_path()` which blocks `..` traversal and paths resolving outside the root. Commands run via `bwrap --ro-bind / / --bind $SANDBOX_ROOT $SANDBOX_ROOT --unshare-net`.

## Code Conventions

- **Error handling**: `anyhow::Result` everywhere. Tool errors use the custom `ToolRunError` wrapper (required by rig's `Tool` trait).
- **Async runtime**: `tokio` multi-threaded. The agent is wrapped in `Arc<Mutex<AppAgent>>`.
- **Tool registration**: Each tool is a struct holding `Arc<ToolCtx>`, implementing `rig::tool::Tool` with const `NAME`, typed `Args` (serde Deserialize), and `Output = String`.
- **Provider dispatch**: `agent.rs` matches `config.provider` to build the appropriate rig client. Unknown providers fall back to OpenAI-compatible with custom `base_url`. The `completions_api()` method is explicitly used (not the newer Responses API).
- **TUI state machine**: `AppState` enum — `Idle → Streaming → ProcessingTools → Idle`. `WaitingConfirmation` pauses for user y/n/a input.

## Gotchas

- **Agent mutex held during streaming**: The `Mutex<AppAgent>` lock is held for the entire LLM stream in TUI mode. This is intentional (no other code needs the lock while streaming) but means you cannot add concurrent agent operations.
- **Two-layer confirmation**: Tool calls go through `TuiHook.on_tool_call()` (rig level). Some tools like `file_editor` also call `ToolCtx.confirm` internally. The `skip_confirmation` flag in `file_editor` prevents double-prompting — when the TUI hook already approved, the inner confirm is skipped.
- **Sandbox requires bubblewrap**: On Linux, `command.rs` shells out to `bwrap`. If bwrap is not installed, command execution will fail. Non-Linux platforms execute unsandboxed.
- **Config format is INI**: `~/.aicli.conf` uses INI format (key=value), not TOML. Loaded via the `config` crate with `FileFormat::Ini`. Legacy `SMTP_SERVER_IP` env var still overrides `smtp_server` for backwards compat.
- **Provider string is case-insensitive**: `agent.rs` lowercases `config.provider` before matching. "Ollama", "OLLAMA", "ollama" all work.
- **Dev profile has opt-level 1**: Not the Rust default of 0. This speeds up dev builds but slightly increases compile time.
- **Files outside sandbox root are inaccessible**: `resolve_sandbox_path` canonicalizes and checks the path starts with `SANDBOX_ROOT`. Files that don't exist yet can't be resolved (canonicalize fails). New file creation only works if the file already exists or the parent directory exists and is within the sandbox.
- **No README synchronization**: `readme.md` references modules like `src/chat.rs` and `src/tools.rs` that no longer exist (refactored into `agent.rs` and `src/tools/`). The README is stale.

## Testing

Tests are inline `#[cfg(test)] mod tests` in each module. Run with `cargo test`. No integration test harness or test fixtures directory. File-edit tests create temporary files in the project root and clean up after themselves. Tests that need the filesystem (path resolution, file read/write) rely on `resolve_sandbox_path` working against the actual cwd.

## Dependencies of Note

- `rig` 0.37 — LLM abstraction with built-in tool calling, streaming, and multi-turn agents
- `ratatui` 0.30 — TUI framework (uses `ratatui::init()` / `ratatui::restore()` API)
- `crossterm` 0.29 — terminal backend
- `patch-apply` 0.8 — unified diff parsing and application
- `tavily` 2.0 — Tavily search API client
- `lettre` 0.11 — SMTP email
- `pithy` 0.1 — extractive text summarization
- `dunce` 1.0 — Windows-compatible canonicalize
