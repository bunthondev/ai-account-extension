mod app;
mod config;
mod mcp;
mod models;
mod oauth;
mod providers;
mod ui;
mod web_auth;

use std::env;
use std::io::{self, stdout};
use std::time::Duration;
use anyhow::Result;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::App;
use crate::config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--mcp" | "-m" => {
                return mcp::run_stdio_mcp_server().await;
            }
            "list" | "ls" => {
                let config = AppConfig::load();
                if config.accounts.is_empty() {
                    println!("No accounts configured. Run `subagent web` or launch `subagent` to add one.");
                    return Ok(());
                }
                println!("| Active | Account Name | Provider | Model | Key / Auth |");
                println!("|:---:|:---|:---|:---|:---|");
                for acc in &config.accounts {
                    let is_active = config.active_account_id.as_deref() == Some(&acc.id);
                    let active_badge = if is_active { "●" } else { " " };
                    println!(
                        "| {} | {} | {} | {} | {} |",
                        active_badge,
                        acc.name,
                        acc.provider.display_name(),
                        acc.model,
                        acc.masked_key()
                    );
                }
                return Ok(());
            }
            "switch" => {
                if args.len() < 3 {
                    eprintln!("Usage: subagent switch <account_name_or_id>");
                    std::process::exit(1);
                }
                let target = &args[2];
                let mut config = AppConfig::load();
                let found = config.accounts.iter().find(|a| {
                    a.id == *target
                        || a.name.eq_ignore_ascii_case(target)
                        || a.oauth_email().map(|e| e.eq_ignore_ascii_case(target)).unwrap_or(false)
                });
                if let Some(acc) = found {
                    let id = acc.id.clone();
                    let name = acc.name.clone();
                    let prov = acc.provider.display_name();
                    let model = acc.model.clone();
                    config.set_active_account(&id)?;
                    println!("Switched active account to: {} ({}) [{}]", name, prov, model);
                } else {
                    eprintln!("No account found matching '{}'. Run `subagent list` to see accounts.", target);
                    std::process::exit(1);
                }
                return Ok(());
            }
            "web" => {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let server = web_auth::WebAuthServer::start(tx)?;
                println!("Web Sign-In portal opened at http://127.0.0.1:{}", server.port);
                println!("Waiting for sign-in (Press Ctrl+C to cancel)...");
                if let Some(account) = rx.recv().await {
                    let mut conf = AppConfig::load();
                    println!("Successfully authenticated: {} ({})", account.name, account.provider.display_name());
                    conf.add_account(account)?;
                }
                server.stop();
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Subagent Multi-Account AI Terminal & MCP Server");
                println!();
                println!("Usage:");
                println!("  subagent               Launch full interactive TUI dashboard");
                println!("  subagent --mcp         Run stdio MCP server for Antigravity");
                println!("  subagent list          List all configured AI accounts");
                println!("  subagent switch <name> Switch active AI account");
                println!("  subagent web           Open browser sign-in portal");
                return Ok(());
            }
            _ => {}
        }
    }

    // Default: Launch TUI dashboard
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let mut app = App::new();
    let mut event_stream = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(40));

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            _ = tick_interval.tick() => {
                app.tick();
            }
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                            app.handle_key(key)?;
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Err(err)) => {
                        eprintln!("Event stream error: {}", err);
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    println!("Goodbye from Multi-Account AI Terminal!");
    Ok(())
}
