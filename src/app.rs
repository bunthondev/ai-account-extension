use std::time::{Duration, Instant};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::config::AppConfig;
use crate::models::{AccountProfile, ChatMessage, ProviderKind, StreamEvent};
use crate::providers::create_client;
use crate::web_auth::{open_browser, WebAuthServer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    ChatInput,
    Sidebar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalState {
    None,
    AddAccount(AddAccountField),
    WebSignIn { port: u16 },
    ConfirmDelete,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddAccountField {
    Provider,
    Name,
    ApiKey,
    Model,
}

impl AddAccountField {
    pub fn next(&self) -> Self {
        match self {
            Self::Provider => Self::Name,
            Self::Name => Self::ApiKey,
            Self::ApiKey => Self::Model,
            Self::Model => Self::Provider,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Provider => Self::Model,
            Self::Name => Self::Provider,
            Self::ApiKey => Self::Name,
            Self::Model => Self::ApiKey,
        }
    }
}

pub struct AddAccountForm {
    pub provider_idx: usize,
    pub name: String,
    pub api_key: String,
    pub model: String,
    pub error_msg: Option<String>,
}

impl Default for AddAccountForm {
    fn default() -> Self {
        let default_prov = ProviderKind::GoogleGemini;
        Self {
            provider_idx: 0,
            name: String::new(),
            api_key: String::new(),
            model: default_prov.default_model().to_string(),
            error_msg: None,
        }
    }
}

impl AddAccountForm {
    pub fn current_provider(&self) -> ProviderKind {
        ProviderKind::all()[self.provider_idx % ProviderKind::all().len()]
    }

    pub fn cycle_provider(&mut self, next: bool) {
        let total = ProviderKind::all().len();
        if next {
            self.provider_idx = (self.provider_idx + 1) % total;
        } else if self.provider_idx == 0 {
            self.provider_idx = total - 1;
        } else {
            self.provider_idx -= 1;
        }
        self.model = self.current_provider().default_model().to_string();
    }
}

pub struct App {
    pub config: AppConfig,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub input_cursor: usize,
    pub focused_pane: FocusedPane,
    pub modal: ModalState,
    pub add_form: AddAccountForm,
    pub selected_account_idx: usize,
    pub chat_scroll: u16,
    pub is_streaming: bool,
    pub current_stream_buffer: String,
    pub stream_rx: Option<UnboundedReceiver<StreamEvent>>,
    pub web_auth_server: Option<WebAuthServer>,
    pub web_auth_rx: Option<UnboundedReceiver<AccountProfile>>,
    pub status_message: Option<(String, Instant)>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let config = AppConfig::load();
        let selected_account_idx = if let Some(active_id) = &config.active_account_id {
            config
                .accounts
                .iter()
                .position(|a| &a.id == active_id)
                .unwrap_or(0)
        } else {
            0
        };

        let mut app = Self {
            config,
            messages: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            focused_pane: FocusedPane::ChatInput,
            modal: ModalState::None,
            add_form: AddAccountForm::default(),
            selected_account_idx,
            chat_scroll: 0,
            is_streaming: false,
            current_stream_buffer: String::new(),
            stream_rx: None,
            web_auth_server: None,
            web_auth_rx: None,
            status_message: None,
            should_quit: false,
        };

        if app.config.accounts.is_empty() {
            app.messages.push(ChatMessage::system(
                "Welcome to Multi-Account AI Terminal!\n\nNo accounts configured yet. Press [Ctrl+W] to sign in via Web Browser, or [Ctrl+A] to enter API keys manually.",
            ));
        } else if let Some(active) = app.config.active_account() {
            app.messages.push(ChatMessage::system(format!(
                "Ready with active account: {} ({}) using model [{}].\nType a message below or press [Tab] to manage accounts, [Ctrl+W] to sign in via Web.",
                active.name,
                active.provider.display_name(),
                active.model
            )));
        }

        app
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status_message = Some((text.into(), Instant::now()));
    }

    pub fn get_status(&self) -> Option<&str> {
        if let Some((msg, time)) = &self.status_message {
            if time.elapsed() < Duration::from_secs(5) {
                return Some(msg.as_str());
            }
        }
        None
    }

    pub fn start_web_sign_in(&mut self) -> Result<()> {
        self.stop_web_sign_in();
        let (tx, rx) = unbounded_channel();
        let server = WebAuthServer::start(tx)?;
        let port = server.port;
        self.web_auth_server = Some(server);
        self.web_auth_rx = Some(rx);
        self.modal = ModalState::WebSignIn { port };
        self.set_status(format!("Web sign-in listening on http://127.0.0.1:{}", port));
        Ok(())
    }

    pub fn stop_web_sign_in(&mut self) {
        if let Some(server) = self.web_auth_server.take() {
            server.stop();
        }
        self.web_auth_rx = None;
        if matches!(self.modal, ModalState::WebSignIn { .. }) {
            self.modal = ModalState::None;
        }
    }

    pub fn tick(&mut self) {
        // Handle LLM streaming response events
        if let Some(rx) = &mut self.stream_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    StreamEvent::Chunk(chunk) => {
                        self.current_stream_buffer.push_str(&chunk);
                    }
                    StreamEvent::Finished => {
                        let active = self.config.active_account().cloned();
                        let label = active.as_ref().map(|a| a.name.clone());
                        let prov = active.as_ref().map(|a| a.provider);

                        let final_content = std::mem::take(&mut self.current_stream_buffer);
                        self.messages.push(ChatMessage::assistant(
                            final_content,
                            label,
                            prov,
                        ));
                        self.is_streaming = false;
                        self.stream_rx = None;
                        self.set_status("Response finished.");
                        break;
                    }
                    StreamEvent::Error(err) => {
                        self.messages.push(ChatMessage::system(format!("Error: {}", err)));
                        self.current_stream_buffer.clear();
                        self.is_streaming = false;
                        self.stream_rx = None;
                        self.set_status(format!("Stream error: {}", err));
                        break;
                    }
                }
            }
        }

        // Handle Web sign-in callback events
        if let Some(rx) = &mut self.web_auth_rx {
            while let Ok(account) = rx.try_recv() {
                let name = account.name.clone();
                let prov = account.provider.display_name();
                let model = account.model.clone();

                if let Err(e) = self.config.add_account(account) {
                    self.set_status(format!("Failed to save account: {}", e));
                } else {
                    self.selected_account_idx = self.config.accounts.len().saturating_sub(1);
                    self.switch_to_selected_account();
                    self.messages.push(ChatMessage::system(format!(
                        "Successfully linked account via Web: {} ({}) using [{}]",
                        name, prov, model
                    )));
                    self.set_status(format!("Added and activated account '{}'", name));
                }
                self.stop_web_sign_in();
                break;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Global hotkeys
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    self.stop_web_sign_in();
                    self.should_quit = true;
                    return Ok(());
                }
                KeyCode::Char('w') => {
                    let _ = self.start_web_sign_in();
                    return Ok(());
                }
                KeyCode::Char('a') => {
                    self.add_form = AddAccountForm::default();
                    self.modal = ModalState::AddAccount(AddAccountField::Provider);
                    return Ok(());
                }
                KeyCode::Char('s') => {
                    self.switch_to_selected_account();
                    return Ok(());
                }
                KeyCode::Char('d') => {
                    if !self.config.accounts.is_empty() {
                        self.modal = ModalState::ConfirmDelete;
                    }
                    return Ok(());
                }
                KeyCode::Char('l') => {
                    self.messages.clear();
                    self.set_status("Chat history cleared.");
                    return Ok(());
                }
                KeyCode::Char('h') => {
                    self.modal = ModalState::Help;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Modal handling
        match self.modal {
            ModalState::WebSignIn { port } => {
                match key.code {
                    KeyCode::Esc => {
                        self.stop_web_sign_in();
                        self.set_status("Web sign-in cancelled.");
                    }
                    KeyCode::Char('o') | KeyCode::Char('O') => {
                        let url = format!("http://127.0.0.1:{}", port);
                        open_browser(&url);
                        self.set_status(format!("Opened browser at {}", url));
                    }
                    _ => {}
                }
                return Ok(());
            }
            ModalState::AddAccount(_) => {
                self.handle_add_account_key(key);
                return Ok(());
            }
            ModalState::ConfirmDelete => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        self.delete_selected_account();
                        self.modal = ModalState::None;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.modal = ModalState::None;
                    }
                    _ => {}
                }
                return Ok(());
            }
            ModalState::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                    self.modal = ModalState::None;
                }
                return Ok(());
            }
            ModalState::None => {}
        }

        // Main screen navigation & input
        match key.code {
            KeyCode::Tab => {
                self.focused_pane = match self.focused_pane {
                    FocusedPane::ChatInput => FocusedPane::Sidebar,
                    FocusedPane::Sidebar => FocusedPane::ChatInput,
                };
            }
            KeyCode::PageUp => {
                self.chat_scroll = self.chat_scroll.saturating_add(5);
            }
            KeyCode::PageDown => {
                self.chat_scroll = self.chat_scroll.saturating_sub(5);
            }
            _ => {
                match self.focused_pane {
                    FocusedPane::ChatInput => self.handle_input_key(key),
                    FocusedPane::Sidebar => self.handle_sidebar_key(key),
                }
            }
        }

        Ok(())
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let total = self.config.accounts.len();
        if total == 0 {
            return;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_account_idx == 0 {
                    self.selected_account_idx = total - 1;
                } else {
                    self.selected_account_idx -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_account_idx = (self.selected_account_idx + 1) % total;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.switch_to_selected_account();
            }
            _ => {}
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input.trim().is_empty() && !self.is_streaming {
                    let prompt = std::mem::take(&mut self.input);
                    self.input_cursor = 0;
                    self.send_prompt(prompt);
                }
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 && self.input_cursor <= self.input.len() {
                    self.input.remove(self.input_cursor - 1);
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input.len() {
                    self.input.remove(self.input_cursor);
                }
            }
            KeyCode::Left => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.input_cursor < self.input.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.input_cursor = 0;
            }
            KeyCode::End => {
                self.input_cursor = self.input.len();
            }
            KeyCode::Char(c) => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += 1;
            }
            _ => {}
        }
    }

    fn handle_add_account_key(&mut self, key: KeyEvent) {
        let current_field = match self.modal {
            ModalState::AddAccount(f) => f,
            _ => return,
        };

        match key.code {
            KeyCode::Esc => {
                self.modal = ModalState::None;
            }
            KeyCode::Tab => {
                let next = current_field.next();
                self.modal = ModalState::AddAccount(next);
            }
            KeyCode::BackTab => {
                let prev = current_field.prev();
                self.modal = ModalState::AddAccount(prev);
            }
            KeyCode::Enter => {
                if current_field == AddAccountField::Model {
                    self.submit_add_account();
                } else {
                    let next = current_field.next();
                    self.modal = ModalState::AddAccount(next);
                }
            }
            _ => {
                match current_field {
                    AddAccountField::Provider => match key.code {
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            let _ = self.start_web_sign_in();
                        }
                        KeyCode::Left | KeyCode::Up => self.add_form.cycle_provider(false),
                        KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') => {
                            self.add_form.cycle_provider(true);
                        }
                        _ => {}
                    },
                    AddAccountField::Name => match key.code {
                        KeyCode::Backspace => {
                            self.add_form.name.pop();
                        }
                        KeyCode::Char(c) => {
                            self.add_form.name.push(c);
                        }
                        _ => {}
                    },
                    AddAccountField::ApiKey => match key.code {
                        KeyCode::Backspace => {
                            self.add_form.api_key.pop();
                        }
                        KeyCode::Char(c) => {
                            self.add_form.api_key.push(c);
                        }
                        _ => {}
                    },
                    AddAccountField::Model => match key.code {
                        KeyCode::Backspace => {
                            self.add_form.model.pop();
                        }
                        KeyCode::Char(c) => {
                            self.add_form.model.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    fn submit_add_account(&mut self) {
        let name = self.add_form.name.trim();
        let api_key = self.add_form.api_key.trim();
        let model = self.add_form.model.trim();

        if name.is_empty() {
            self.add_form.error_msg = Some("Please provide an account name/label".to_string());
            return;
        }
        if api_key.is_empty() {
            self.add_form.error_msg = Some("API key cannot be empty".to_string());
            return;
        }

        let prov = self.add_form.current_provider();
        let account = AccountProfile::new(
            name,
            prov,
            api_key,
            if model.is_empty() { None } else { Some(model.to_string()) },
            None,
        );

        let acct_name = account.name.clone();
        if let Err(e) = self.config.add_account(account) {
            self.add_form.error_msg = Some(format!("Failed to save config: {}", e));
            return;
        }

        self.selected_account_idx = self.config.accounts.len().saturating_sub(1);
        self.modal = ModalState::None;
        self.set_status(format!("Added account '{}'", acct_name));
    }

    pub fn switch_to_selected_account(&mut self) {
        if let Some(target) = self.config.accounts.get(self.selected_account_idx) {
            let id = target.id.clone();
            let name = target.name.clone();
            let prov = target.provider.display_name();
            let model = target.model.clone();

            if let Err(e) = self.config.set_active_account(&id) {
                self.set_status(format!("Error activating account: {}", e));
            } else {
                self.set_status(format!("Switched to {} ({}) [{}]", name, prov, model));
                self.messages.push(ChatMessage::system(format!(
                    "Switched active profile to: {} ({}) using model [{}]",
                    name, prov, model
                )));
            }
        }
    }

    pub fn delete_selected_account(&mut self) {
        if let Some(target) = self.config.accounts.get(self.selected_account_idx) {
            let id = target.id.clone();
            let name = target.name.clone();
            if let Err(e) = self.config.remove_account(&id) {
                self.set_status(format!("Failed to delete: {}", e));
            } else {
                if self.selected_account_idx >= self.config.accounts.len() && !self.config.accounts.is_empty() {
                    self.selected_account_idx = self.config.accounts.len() - 1;
                }
                self.set_status(format!("Removed account '{}'", name));
            }
        }
    }

    pub fn send_prompt(&mut self, prompt: String) {
        let active = match self.config.active_account() {
            Some(a) => a.clone(),
            None => {
                self.messages.push(ChatMessage::system(
                    "No active account configured. Please press [Ctrl+W] to sign in via Web or [Ctrl+A] to add an account.",
                ));
                return;
            }
        };

        self.messages.push(ChatMessage::user(prompt));
        self.is_streaming = true;
        self.current_stream_buffer.clear();
        self.chat_scroll = 0;

        let (tx, rx) = unbounded_channel();
        self.stream_rx = Some(rx);

        let client = create_client(&active);
        let history = self.messages.clone();

        tokio::spawn(async move {
            if let Err(e) = client.stream_completion(&history, tx.clone()).await {
                let _ = tx.send(StreamEvent::Error(e.to_string()));
            }
        });

        self.set_status(format!("Streaming from {}...", active.name));
    }
}
