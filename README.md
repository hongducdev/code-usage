# CodeUsage

> A compact desktop dashboard for tracking AI coding-agent quotas, local usage, and active sessions from one place.

CodeUsage is a privacy-conscious system-tray app built with Tauri 2 and Rust. It discovers supported coding tools already installed on your computer, refreshes available quota data, estimates local Codex and Claude usage, and includes an always-on-top Agent Pet for lightweight activity monitoring.

## Highlights

- Track quota and account usage across multiple AI coding providers.
- Discover local credentials and session footprints automatically.
- Estimate Codex and Claude token usage and cost from local logs.
- View today and 30-day usage, latest request tokens, top model, and a 14-day activity chart.
- Refresh every provider together or update one provider at a time.
- Keep the dashboard available from the system tray.
- Monitor active Codex and Claude sessions with the draggable Agent Pet.
- Store OpenRouter and Z.ai API keys in the operating-system credential vault.

## Provider support

| Provider | Credential discovery | Live quota refresh | Status |
| --- | --- | --- | --- |
| Codex | `$CODEX_HOME/auth.json` or `~/.codex/auth.json` | ChatGPT usage endpoint | Supported |
| Claude | `$CLAUDE_CONFIG_DIR/.credentials.json` or `~/.claude/.credentials.json` | Claude OAuth usage endpoint | Supported |
| OpenRouter | OS credential vault or `OPENROUTER_API_KEY` | Credits and API-key endpoints | Supported |
| Z.ai | OS credential vault or `ZAI_API_KEY` | Coding-plan quota endpoint | Supported |
| GitHub Copilot | GitHub CLI authentication | Copilot quota endpoint | Experimental |
| Cursor | Local state database | Cursor Connect-RPC endpoint | Experimental |
| Devin | Local credentials | Devin Connect-RPC endpoint | Experimental |
| Grok | `$GROK_HOME/auth.json` or `~/.grok/auth.json` | Grok billing endpoint | Experimental |
| OpenCode | Platform data directory | Credential discovery only | Experimental |
| Antigravity | Running app and local state | Credential discovery only | Experimental |

Some providers use undocumented vendor endpoints that may change without notice.

## Privacy and security

CodeUsage processes local usage data on your device. Local scans report session counts, aggregate token and cost estimates, and recent activity metadata; user prompts are not displayed in the dashboard or sent to CodeUsage servers.

OpenRouter and Z.ai keys entered in Settings are stored through the native operating-system credential service. They are not written to project files. Provider refresh requests are sent directly from the desktop app to the relevant provider endpoints.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) with Cargo
- Platform prerequisites for [Tauri 2](https://v2.tauri.app/start/prerequisites/)
- An authenticated installation or API key for each provider you want to track
- GitHub CLI authenticated with `gh auth login` to use Copilot discovery

## Run from source

Clone the repository and start the Tauri app:

```powershell
git clone https://github.com/hongducdev/code-usage.git
cd code-usage
cargo run --manifest-path src-tauri/Cargo.toml
```

The main window can be hidden without quitting. Use the tray icon to reopen it, refresh providers, show or hide Agent Pet, or exit.

## Build

Check the Rust application:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
```

Create the configured Windows NSIS bundle:

```powershell
cargo install tauri-cli --version "^2"
cargo tauri build --config src-tauri/tauri.conf.json
```

The repository currently configures an NSIS installer for Windows. Additional bundle targets can be added in `src-tauri/tauri.conf.json`.

## Project structure

```text
.
|-- ui/                   # HTML, CSS, JavaScript, and provider logos
|-- src-tauri/            # Rust backend and Tauri configuration
|   |-- src/providers.rs  # Provider discovery and quota connectors
|   |-- src/scanner.rs    # Local session and usage scanning
|   `-- src/main.rs       # Commands, windows, tray, and app lifecycle
|-- bintest/              # Rust binary for connector experiments
`-- design.md             # Product and interface design notes
```

## Notes

- Cost figures derived from local logs are estimates based on token counts and the bundled model-rate table.
- Quota percentages and reset times come from provider usage APIs when available.
- Provider names and logos are trademarks of their respective owners.
- Provider SVG marks are reused from the MIT-licensed [OpenUsage](https://github.com/robinebers/openusage) project.

## Contributing

Issues and focused pull requests are welcome. When changing a connector, avoid logging credentials or prompt contents and verify that provider failures degrade gracefully.
