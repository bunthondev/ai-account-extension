import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as cp from 'child_process';
import * as vscode from 'vscode';
import { AccountProfile, AppConfig } from './types';

export class ConfigManager {
  private configPath: string;
  private fileWatcher?: fs.FSWatcher;
  private changeEmitter = new vscode.EventEmitter<AppConfig>();
  public readonly onDidChangeConfig = this.changeEmitter.event;

  constructor() {
    this.configPath = this.resolveConfigPath();
    this.initWatcher();
  }

  public resolveConfigPath(): string {
    const isMac = process.platform === 'darwin';
    if (isMac) {
      const macPath = path.join(
        os.homedir(),
        'Library',
        'Application Support',
        'com.subagent.terminal',
        'config.json'
      );
      if (fs.existsSync(macPath)) {
        return macPath;
      }
    }

    const xdgPath = path.join(os.homedir(), '.config', 'subagent', 'config.json');
    if (fs.existsSync(xdgPath)) {
      return xdgPath;
    }

    if (isMac) {
      return path.join(
        os.homedir(),
        'Library',
        'Application Support',
        'com.subagent.terminal',
        'config.json'
      );
    }
    return xdgPath;
  }

  public getBinaryPath(): string | undefined {
    const configuredPath = vscode.workspace.getConfiguration('subagent').get<string>('binaryPath');
    if (configuredPath && fs.existsSync(configuredPath)) {
      return configuredPath;
    }

    const candidatePaths = [
      path.join(os.homedir(), '.gemini', 'config', 'plugins', 'multi-account-ai', 'bin', 'subagent'),
      path.join(__dirname, '..', '..', 'target', 'release', 'subagent'),
      '/Users/puchan/dev/working/luma-system/projects/subagent/target/release/subagent',
    ];

    for (const candidate of candidatePaths) {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }

    try {
      const res = cp.execSync('which subagent', { encoding: 'utf-8' }).trim();
      if (res && fs.existsSync(res)) {
        return res;
      }
    } catch {
      // not in PATH
    }

    return undefined;
  }

  public readConfig(): AppConfig {
    try {
      if (fs.existsSync(this.configPath)) {
        const raw = fs.readFileSync(this.configPath, 'utf-8');
        const parsed = JSON.parse(raw);
        return {
          active_account_id: parsed.active_account_id || null,
          accounts: Array.isArray(parsed.accounts) ? parsed.accounts : [],
        };
      }
    } catch (err) {
      console.error(`Failed to read subagent config at ${this.configPath}:`, err);
    }
    return { active_account_id: null, accounts: [] };
  }

  public saveConfig(config: AppConfig): boolean {
    try {
      const dir = path.dirname(this.configPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.configPath, JSON.stringify(config, null, 2), 'utf-8');
      this.changeEmitter.fire(config);
      return true;
    } catch (err) {
      console.error(`Failed to write subagent config at ${this.configPath}:`, err);
      return false;
    }
  }

  public setActiveAccount(accountIdOrName: string): { success: boolean; account?: AccountProfile; keychainUpdated: boolean } {
    const config = this.readConfig();
    const found = config.accounts.find(
      (a) => a.id === accountIdOrName || a.name.toLowerCase() === accountIdOrName.toLowerCase()
    );

    if (!found) {
      return { success: false, keychainUpdated: false };
    }

    config.active_account_id = found.id;
    const ok = this.saveConfig(config);

    // Sync to macOS Keychain if it's a Google / Gemini account
    let keychainUpdated = false;
    if (found.provider === 'gemini') {
      keychainUpdated = this.syncKeychainForAccount(found);
    }

    // Trigger switch in subagent binary CLI
    const bin = this.getBinaryPath();
    if (bin) {
      try {
        cp.execFile(bin, ['switch', found.id], (err) => {
          if (err) {
            console.warn('subagent CLI switch exited with error:', err);
          }
        });
      } catch {
        // ignore CLI error
      }
    }

    return { success: ok, account: found, keychainUpdated };
  }

  public syncKeychainForAccount(account: AccountProfile): boolean {
    if (process.platform !== 'darwin') {
      return false;
    }

    const profilesDir = path.join(os.homedir(), '.gemini', 'antigravity', 'profiles');
    let tokenStr: string | null = null;
    let email = '';

    const authAny = account.auth_type as any;
    if (authAny && typeof authAny === 'object' && authAny.oauth?.email) {
      email = authAny.oauth.email;
    } else if (account.name.includes('@')) {
      const match = account.name.match(/[\w.-]+@[\w.-]+\.\w+/);
      if (match) email = match[0];
    }

    // 1. Check existing profiles in ~/.gemini/antigravity/profiles/
    if (fs.existsSync(profilesDir)) {
      const files = fs.readdirSync(profilesDir);
      for (const f of files) {
        if (f.endsWith('.token')) {
          try {
            const raw = fs.readFileSync(path.join(profilesDir, f), 'utf-8').trim();
            if (raw.startsWith('go-keyring-base64:')) {
              const decoded = Buffer.from(
                raw.slice('go-keyring-base64:'.length),
                'base64'
              ).toString('utf-8');
              const parsed = JSON.parse(decoded);
              if (
                parsed.id_token_email &&
                email &&
                parsed.id_token_email.toLowerCase() === email.toLowerCase()
              ) {
                tokenStr = raw;
                break;
              }
            }
          } catch {
            // ignore malformed token file
          }
        }
      }
    }

    // 2. If no token found from file, construct one from OAuth info
    if (!tokenStr && authAny && typeof authAny === 'object' && authAny.oauth) {
      const oauth = authAny.oauth;
      const payload = {
        token: {
          access_token: account.api_key,
          token_type: 'Bearer',
          refresh_token: oauth.refresh_token,
          expiry: oauth.expires_at || new Date(Date.now() + 3600000).toISOString(),
          expires_in: 3599,
        },
        auth_method: 'consumer',
        id_token_email: oauth.email || email,
        writer_id: 'subagent',
      };
      tokenStr = 'go-keyring-base64:' + Buffer.from(JSON.stringify(payload)).toString('base64');

      try {
        if (!fs.existsSync(profilesDir)) {
          fs.mkdirSync(profilesDir, { recursive: true, mode: 0o700 });
        }
        const profileName = email
          ? email.split('@')[0]
          : account.name.replace(/[^a-zA-Z0-9_-]/g, '_');
        fs.writeFileSync(path.join(profilesDir, `${profileName}.token`), tokenStr, {
          mode: 0o600,
        });
      } catch (err) {
        console.warn('Failed to save profile token file:', err);
      }
    }

    // 3. Write into macOS Keychain
    if (tokenStr) {
      try {
        cp.execFileSync('/usr/bin/security', [
          'add-generic-password',
          '-U',
          '-s',
          'gemini',
          '-a',
          'antigravity',
          '-w',
          tokenStr,
        ]);
        return true;
      } catch (err) {
        console.error('Failed to write to macOS Keychain:', err);
      }
    }

    return false;
  }

  public getCurrentKeychainEmail(): string | null {
    if (process.platform !== 'darwin') return null;
    try {
      const raw = cp
        .execFileSync(
          '/usr/bin/security',
          ['find-generic-password', '-s', 'gemini', '-a', 'antigravity', '-w'],
          { encoding: 'utf-8' }
        )
        .trim();
      if (raw.startsWith('go-keyring-base64:')) {
        const json = Buffer.from(raw.slice('go-keyring-base64:'.length), 'base64').toString(
          'utf-8'
        );
        const parsed = JSON.parse(json);
        return parsed.id_token_email || null;
      }
    } catch {
      // not available or locked
    }
    return null;
  }

  public addAccount(account: AccountProfile): boolean {
    const config = this.readConfig();
    config.accounts.push(account);
    if (!config.active_account_id || config.accounts.length === 1) {
      config.active_account_id = account.id;
    }
    return this.saveConfig(config);
  }

  public deleteAccount(accountId: string): boolean {
    const config = this.readConfig();
    const prevLen = config.accounts.length;
    config.accounts = config.accounts.filter((a) => a.id !== accountId);

    if (config.accounts.length === prevLen) {
      return false;
    }

    if (config.active_account_id === accountId) {
      config.active_account_id = config.accounts.length > 0 ? config.accounts[0].id : null;
    }

    return this.saveConfig(config);
  }

  public getActiveAccount(): AccountProfile | undefined {
    const config = this.readConfig();
    if (!config.active_account_id) {
      return config.accounts[0];
    }
    return config.accounts.find((a) => a.id === config.active_account_id) || config.accounts[0];
  }

  private initWatcher(): void {
    const dir = path.dirname(this.configPath);
    if (!fs.existsSync(dir)) {
      try {
        fs.mkdirSync(dir, { recursive: true });
      } catch {
        return;
      }
    }

    let debounceTimer: NodeJS.Timeout | null = null;
    try {
      this.fileWatcher = fs.watch(dir, (_event, filename) => {
        if (!filename || filename === path.basename(this.configPath)) {
          if (debounceTimer) {
            clearTimeout(debounceTimer);
          }
          debounceTimer = setTimeout(() => {
            const updated = this.readConfig();
            this.changeEmitter.fire(updated);
          }, 150);
        }
      });
    } catch (err) {
      console.warn('Unable to watch subagent config directory:', err);
    }
  }

  public dispose(): void {
    if (this.fileWatcher) {
      this.fileWatcher.close();
    }
    this.changeEmitter.dispose();
  }
}
