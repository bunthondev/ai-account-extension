use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::models::{AccountProfile, ProviderKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub active_account_id: Option<String>,
    pub accounts: Vec<AccountProfile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_account_id: None,
            accounts: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("com", "subagent", "terminal") {
            let config_dir = proj_dirs.config_dir();
            config_dir.join("config.json")
        } else {
            PathBuf::from("config.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).context("Failed to write config file")?;
        Ok(())
    }

    pub fn active_account(&self) -> Option<&AccountProfile> {
        self.active_account_id
            .as_ref()
            .and_then(|id| self.accounts.iter().find(|a| &a.id == id))
            .or_else(|| self.accounts.first())
    }

    pub fn add_account(&mut self, account: AccountProfile) -> Result<()> {
        let id = account.id.clone();
        self.accounts.push(account);
        if self.active_account_id.is_none() {
            self.active_account_id = Some(id);
        }
        self.save()
    }

    pub fn remove_account(&mut self, id: &str) -> Result<()> {
        self.accounts.retain(|a| a.id != id);
        if self.active_account_id.as_deref() == Some(id) {
            self.active_account_id = self.accounts.first().map(|a| a.id.clone());
        }
        self.save()
    }

    pub fn set_active_account(&mut self, id: &str) -> Result<()> {
        if self.accounts.iter().any(|a| a.id == id) {
            self.active_account_id = Some(id.to_string());
            self.save()?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn accounts_for_provider(&self, provider: ProviderKind) -> Vec<&AccountProfile> {
        self.accounts.iter().filter(|a| a.provider == provider).collect()
    }
}
