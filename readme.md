# AI CLI

## Description

AI CLI is a Rust application that acts as a provider-agnostic AI assistant within a sandboxed multi-platform terminal environment. It supports any OpenAI-compatible API to assist with coding tasks, file operations, online searches, email sending, and shell commands. The application takes initiative to provide solutions, execute commands, and analyze results without explicit user confirmation, unless the action is ambiguous or potentially destructive.

## Functionality

*   **Chat Interface:** Provides a command-line interface for interacting with AI models.
*   **Provider Agnostic:** Works with any OpenAI-compatible API (Google Gemini, OpenAI, local LLMs, etc.).
*   **Tool Execution:** Executes system commands using the `execute_command` function, allowing the AI to interact with the file system and other system utilities.
*   **Online Search:** Performs online searches using the `search_online` function, enabling the AI to retrieve up-to-date information from the web.
*   **Email Sending:** Sends emails using the `send_email` function, allowing the AI to send notifications or reports.
*   **Conversation History:** Maintains a conversation history to provide context for the AI model.
*   **Ctrl+C Handling:** Gracefully shuts down the application and cleans up resources when Ctrl+C is pressed.

## Modules

*   `src/main.rs`: Application entry point, argument parsing, and interactive loop.
*   `src/config.rs`: Configuration loading from `~/.aicli.conf` and environment variables (prefixed with `AICLI_`).
*   `src/chat.rs`: LLM API client with conversation history management, retry logic, and tool definitions.
*   `src/tools.rs`: Tool call dispatch, response display with Markdown rendering, and output normalization.
*   `src/search.rs`: Online search functionality using the Tavily Search API.
*   `src/command.rs`: System command execution with sandboxing (bubblewrap on Linux).
*   `src/email.rs`: Email sending functionality with SMTP support.
*   `src/alpha_vantage.rs`: Integration with the Alpha Vantage API for financial data.
*   `src/file_edit.rs`: File editing capabilities (read, write, search, search and replace, apply diff) with path validation.
*   `src/scrape.rs`: URL content scraping with summarization.
*   `src/shell.rs`: Shell detection and interactive shell mode.
*   `src/sandbox.rs`: Sandbox root directory management.
*   `src/patch.rs`: Patch/diff application utility.
*   `src/http.rs`: Shared async HTTP client.
*   `src/utils.rs`: Shared utilities (logging, text summarization, retry, user confirmation).

## Configuration Setup

To run AI CLI, you need to set up a `.aicli.conf` file in your home directory with the following variables:

### Basic Configuration

```
# AI Provider Configuration (Required)
API_BASE_URL=https://generativelanguage.googleapis.com
API_VERSION=v1beta
MODEL=gemini-1.5-flash
API_KEY=your_api_key_here
```

### SMTP Configuration (Optional)

```
SMTP_SERVER_IP=localhost
SMTP_USERNAME=
SMTP_PASSWORD=
DESTINATION_EMAIL=
SENDER_EMAIL=
```

### Search APIs (Optional)

```
TAVILY_API_KEY=
ALPHA_VANTAGE_API_KEY=
```

## Example Configurations

### For Google Gemini:
```bash
API_BASE_URL=https://generativelanguage.googleapis.com
API_VERSION=v1beta
MODEL=gemini-1.5-flash
API_KEY=your_gemini_api_key_here
```

### For OpenAI:
```bash
API_BASE_URL=https://api.openai.com
API_VERSION=v1
MODEL=gpt-4
API_KEY=sk-your_openai_api_key_here
```

### For Local LLM (Ollama):
```bash
API_BASE_URL=http://localhost:11434
API_VERSION=v1
MODEL=llama3
API_KEY=
```

### For Other OpenAI-Compatible APIs:
```bash
API_BASE_URL=https://your-provider.com
API_VERSION=v1
MODEL=your-model-name
API_KEY=your_api_key_here
```

## Configuration Parameters

*   `API_BASE_URL`: The base URL of the AI provider's API endpoint
*   `API_VERSION`: The API version to use (e.g., v1, v1beta)
*   `MODEL`: The model name to use (e.g., gemini-2.5-flash, gpt-4, llama3)
*   `API_KEY`: Your API key for authentication
*   `SMTP_SERVER_IP`: The IP address or hostname of the SMTP server (defaults to localhost if not specified)
*   `SMTP_USERNAME`: Username for SMTP authentication (optional, required for non-localhost servers)
*   `SMTP_PASSWORD`: Password for SMTP authentication (optional, required for non-localhost servers)
*   `DESTINATION_EMAIL`: The email address to which the `send_email` function will send emails
*   `SENDER_EMAIL`: The email address to use as the sender (optional, defaults to DESTINATION_EMAIL)
*   `TAVILY_API_KEY`: Your API key for the Tavily Search API
*   `ALPHA_VANTAGE_API_KEY`: Your API key for the Alpha Vantage API
*   Environment variables can override config file values by prefixing with `AICLI_`. For example, `AICLI_API_KEY` overrides `API_KEY`, `AICLI_MODEL` overrides `MODEL`.

## Usage

1.  Clone the repository:

    ```bash
    git clone <repository_url>
    cd ai-cli
    ```

2.  Create a `.aicli.conf` file in your home directory and set the required environment variables as described in the Configuration Setup section.

3.  Run the application:

    ```bash
    cargo run
    ```

4.  Chat with the AI by typing messages in the command-line interface. Use `!command` to run shell commands directly (e.g., `!ls` or `!dir`). Type `exit` to quit or `clear` to reset the conversation.

## Migration from Previous Version

If you were using the previous version, you can migrate your configuration:

1. Rename your existing `.gemini.conf` to `.aicli.conf`:
   ```bash
   mv ~/.gemini.conf ~/.aicli.conf
   ```

2. Add the new required fields to your `.aicli.conf`:
   ```bash
   API_BASE_URL=https://generativelanguage.googleapis.com
   API_VERSION=v1beta
   MODEL=gemini-1.5-flash
   ```

3. Keep your existing `API_KEY` (renamed from `GEMINI_API_KEY`)

## Supported Providers

AI CLI is designed to work with any OpenAI-compatible API. The following providers have been tested:

*   **Google Gemini**: Full support with tool calling
*   **OpenAI**: Full support with tool calling
*   **Local LLMs (Ollama)**: Basic support (may require adjustments for tool calling)

### Provider-Specific Notes

#### Google Gemini
- Uses Bearer header authentication via the `async-openai` crate
- Endpoint format: `{base_url}/{version}/chat/completions`
- Full tool calling support

#### OpenAI
- Uses header authentication (`Authorization: Bearer API_KEY`)
- Endpoint format: `{base_url}/{version}/chat/completions`
- Full tool calling support

#### Local LLMs (Ollama)
- May not require authentication
- Endpoint format: `{base_url}/{version}/chat/completions`
- Tool calling support varies by model

## Debug Mode

Run with the `--debug` flag to log configuration details and API call information to `debug.log`:

```bash
cargo run -- --debug
```

This will log to `debug.log` in the current directory:
- AI provider configuration (API base URL, version, model, masked API key)
- API endpoint being used
- SMTP configuration (server, credentials, email addresses)
- All LLM API calls and responses
- Tool calls and their results
- Command execution and output

## Troubleshooting

### Configuration Issues
- Ensure `~/.aicli.conf` exists and contains the required fields
- Check that your API key is valid and has the correct format
- Verify the API base URL is correct for your provider

### API Connection Issues
- Check your internet connection
- Verify the API endpoint is accessible
- Ensure your API key has sufficient credits/permissions

### Tool Calling Issues
- Some providers may have limited tool calling support
- Check the provider's documentation for compatibility
- Try using a different model if available

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues for bugs and feature requests.

## License

MIT License