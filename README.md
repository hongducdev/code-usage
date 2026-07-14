# CodeUsage

Cross-platform tray dashboard for AI coding-agent quotas. The app is built with Tauri 2 and a Rust provider core. It follows the provider vocabulary and credential-discovery approach documented by [OpenUsage](https://github.com/robinebers/openusage), while remaining an independent implementation focused on Windows, macOS, and Linux.

## Run

```powershell
cargo run --manifest-path src-tauri/Cargo.toml
```

The app opens as a compact window and stays in the system tray when closed. Use the tray menu to show it, refresh all providers, or quit.

Agent Pet is a small always-on-top companion that can be dragged anywhere across connected displays. It watches recent local Codex and Claude session events, shows how many agents are working, and expands into a multi-agent activity list without losing its current position. Codex and Claude have distinct pet styles and animations for idle, active work, approval requests, and dragging. Open or hide it from the main title bar or the system tray. The pet summarizes assistant updates and tool categories, but never displays user prompts.

At startup and on every full refresh, CodeUsage scans local provider footprints. It reports only file/session counts and the most recent activity timestamp; prompt contents are never loaded into the UI or sent over the network.

Codex and Claude session logs are also summarized locally into today/30-day estimated cost, token totals, latest request tokens, top model, and a 14-day activity chart. Cost is an estimate from token counts and the model-rate table; quota percentages and reset times still come from each provider's usage API.

## Provider support

| Provider | Discovery | Live refresh |
|---|---|---|
| Codex | `$CODEX_HOME/auth.json` or `~/.codex/auth.json` | ChatGPT usage endpoint |
| Claude | `$CLAUDE_CONFIG_DIR/.credentials.json` or `~/.claude/.credentials.json` | Claude OAuth usage endpoint |
| Copilot | GitHub CLI (`gh auth token`) | Copilot quota endpoint |
| OpenRouter | OS keyring or `OPENROUTER_API_KEY` | Credits and key endpoints |
| Z.ai | OS keyring or `ZAI_API_KEY` | Coding-plan quota endpoint |
| Grok | `$GROK_HOME/auth.json` or `~/.grok/auth.json` | Grok billing endpoint |
| Devin | `~/.local/share/devin/credentials.toml` | Connect-RPC quota endpoint |
| OpenCode | platform data directory `opencode/auth.json` | Credential discovery; mapper is staged |
| Cursor | Cursor local state database | Discovery; Connect-RPC connector is staged |
| Antigravity | running app / local state database | Discovery; local language-server probing is staged |

API keys entered in Settings are stored in the operating-system credential vault, not in project files. Several providers rely on undocumented vendor endpoints and can change without notice.

Provider SVG marks are reused from the MIT-licensed OpenUsage project. Product names and logos remain trademarks of their respective owners.
