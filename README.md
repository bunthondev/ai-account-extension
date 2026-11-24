# Multi-Account AI Terminal & Antigravity IDE Extension (`subagent`)

A blazing fast, full-screen Rust terminal (TUI) application and official **Antigravity IDE Extension** for managing multiple AI accounts across **Google Gemini**, **Anthropic Claude**, and **OpenAI / Codex**. Features Google OAuth 2.0 authentication, instant profile switching, status bar indicators, Activity Bar account navigation, and real-time Model Context Protocol (MCP) tool integration for Antigravity agents.

---

## 🧩 Antigravity IDE Extension (`antigravity-subagent`)

The workspace includes a complete native extension for **Antigravity IDE**:

- **Live AI Quota & Usage Dashboard**: Click directly on the status bar or sidebar to open the full interactive Quotas & Usage dashboard showing prompt credits, flow credits, tier details, and model rate limits.
- **Status Bar Integration**: Displays the active account, model, and live quota remaining (e.g. `$(graph) Quota: 100% (500 cr)`). Click either item to switch accounts or open the usage dashboard.
- **Activity Bar View ("AI Accounts")**: An interactive tree view in the sidebar showing your configured accounts categorized by provider (Gemini, Claude, OpenAI), displaying active badges, models, and inline buttons to activate or delete accounts. Also displays a dedicated **AI Quotas & Credits** section.
- **Google OAuth & Keychain Sync**: Seamlessly syncs Google OAuth credentials directly with macOS Keychain (`gemini / antigravity`) so the Antigravity Agent can switch accounts.
- **Live State Synchronization**: Watches account configuration changes in real time so changes made in CLI, TUI, or browser are immediately reflected in Antigravity IDE without reload.
- **Antigravity MCP Auto-Registration**: Configures the `multi-account-ai` MCP server in Antigravity IDE (`~/Library/Application Support/Antigravity IDE/User/mcp.json`) so your AI assistant can query and switch accounts automatically.

### 📦 1-Click Build & Install to Antigravity IDE

```bash
# Build Rust binary, compile extension, package VSIX, and install to Antigravity IDE
./install-extension.sh
```

Or via npm:
```bash
npm run install:ext
```

---

## ⌨️ Antigravity IDE Commands (`Cmd+Shift+P`)

| Command | Description |
| :--- | :--- |
| `Multi-Account AI: View AI Quotas & Usage` | Open interactive AI Quota, Prompt/Flow Credits, and Model Limits dashboard |
| `Multi-Account AI: Switch Active Account` | QuickPick menu to switch active AI account or trigger actions |
| `Multi-Account AI: Web Sign-In` | Launch Google OAuth 2.0 / API key browser portal at `http://127.0.0.1:8085` |
| `Multi-Account AI: Open TUI Terminal Dashboard` | Open the interactive Ratatui dashboard in an IDE terminal |
| `Multi-Account AI: Add Account Profile` | Step-by-step modal to add Gemini, Claude, or OpenAI accounts |
| `Multi-Account AI: Delete Account Profile` | Select and remove an account profile |
| `Multi-Account AI: Refresh Accounts` | Force refresh status bar, sidebar tree view, and live quotas |
| `Multi-Account AI: Sync MCP Server in Antigravity IDE` | Verify and update Antigravity IDE `mcp.json` server config |
| `Multi-Account AI: Reload Antigravity Window & Agent` | Reloads IDE window so language server reconnects with new credentials |

---

## 🚀 Terminal & Core Features

- **Google OAuth 2.0 Authentication**: Sign into your Google account directly in your browser. Exchanges authorization codes for access and refresh tokens, fetches user info (`user@gmail.com`), and automatically connects to Gemini API with Bearer token auth.
- **Web Browser Sign-In (`Ctrl+W`)**: Start a local authentication web server with a single keystroke. Automatically opens your browser at a local portal (`http://127.0.0.1:8085`) featuring Google OAuth 2.0 and direct API key input tabs.
- **Multi-Account Profile Management**: Add multiple accounts for each provider (e.g., `Google (work@company.com)`, `Personal Gemini`, `Claude Team`, `OpenAI Codex`).
- **Instant Account Switching**: Switch active accounts with hotkeys or via the sidebar menu. Conversation history tracks which provider and account generated each response.
- **Unified Streaming Architecture**: Real-time server-sent events (SSE) streaming for all three providers:
  - **Google Gemini** (`gemini-2.5-flash`, `gemini-1.5-pro`, etc. — supports both OAuth Bearer and API keys)
  - **Anthropic Claude** (`claude-3-5-sonnet-20241022`, `claude-3-5-haiku-20241022`, etc.)
  - **OpenAI / Codex** (`gpt-4o`, `gpt-4o-mini`, `o3-mini`, etc.)
- **Interactive TUI Interface**:
  - Split layout with Accounts sidebar, Conversation history, and Prompt input bar.
  - Interactive "Add Account" modal with live provider selection and masked API key entry.
  - Web sign-in progress modal with live link and re-open shortcut.
- **Secure Local Persistence**: Credentials and account configurations are saved locally in `~/Library/Application Support/com.subagent.terminal/config.json`.

---

## 🌐 How to Sign In via Google OAuth 2.0

1. Press **`Ctrl + W`** inside the terminal (or run `Multi-Account AI: Web Sign-In` in Antigravity IDE).
2. The auth server starts and opens `http://127.0.0.1:8085` in your browser.
3. On the **Google OAuth 2.0** tab:
   - Enter your Google Cloud OAuth 2.0 Client ID (from Google Cloud Console -> APIs & Services -> Credentials -> Create OAuth client ID -> Desktop App).
   - Click **Sign in with Google**.
4. In Google's consent screen, authorize access to your Google account.
5. Google redirects back to `http://127.0.0.1:8085/oauth/callback`.
6. The profile is automatically linked as `Google (<email>)`, configured with Gemini, and activated!

---

## ⌨️ Terminal Keybindings & Controls

| Keybinding | Action |
| :--- | :--- |
| `Ctrl + W` | **Sign in from Web Browser** (OAuth 2.0 or API key web portal) |
| `Tab` | Toggle focus between **Chat Input** and **Accounts Sidebar** |
| `Ctrl + A` | Open the manual **Add Account** dialog |
| `Ctrl + S` / `Enter` (Sidebar) | Switch active account to the highlighted profile |
| `Ctrl + D` | Delete the highlighted account |
| `Ctrl + L` | Clear conversation history |
| `Ctrl + H` | Open Help modal |
| `PageUp` / `PageDown` | Scroll conversation history |
| `Enter` (Chat Input) | Send message to active LLM provider |
| `Ctrl + C` | Exit application |

---

## 🛠️ CLI Commands

```bash
# Launch full interactive TUI dashboard
subagent

# Run stdio MCP server for Antigravity AI
subagent --mcp

# List all configured accounts
subagent list

# Switch active account
subagent switch "Gemini Pro"

# Open Web Sign-in portal
subagent web
```
