import * as vscode from 'vscode';
import * as cp from 'child_process';
import { ConfigManager } from './configManager';
import { StatusBarManager } from './statusBar';
import { AccountsTreeDataProvider, AccountTreeItem } from './treeProvider';
import { McpManager } from './mcpManager';
import { UsageService } from './usageService';
import { UsageWebviewManager } from './usageWebview';
import { AccountProfile, ProviderKind, PROVIDER_INFO } from './types';

let webServerProcess: cp.ChildProcess | null = null;

export function activate(context: vscode.ExtensionContext): void {
  const configManager = new ConfigManager();
  const usageService = new UsageService();
  const usageWebview = new UsageWebviewManager(usageService, context.extensionUri);
  const statusBar = new StatusBarManager(configManager, usageService);
  const treeProvider = new AccountsTreeDataProvider(configManager, usageService);
  const mcpManager = new McpManager(configManager);

  // Sync MCP server definition into Antigravity IDE
  mcpManager.syncMcpServer(false);

  // Register Tree View
  const treeView = vscode.window.createTreeView('subagent.accountsView', {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });

  // Helper to handle switch results
  async function handleAccountSwitch(res: {
    success: boolean;
    account?: AccountProfile;
    keychainUpdated: boolean;
  }) {
    if (!res.success || !res.account) {
      vscode.window.showErrorMessage('Failed to switch account.');
      return;
    }

    if (res.keychainUpdated) {
      const action = await vscode.window.showInformationMessage(
        `Switched active account to: ${res.account.name}. macOS Keychain credentials updated! Reload window to apply to the Antigravity Agent.`,
        'Reload Window to Apply to Agent'
      );
      if (action === 'Reload Window to Apply to Agent') {
        vscode.commands.executeCommand('workbench.action.reloadWindow');
      }
    } else if (res.account.provider !== 'gemini') {
      vscode.window.showInformationMessage(
        `Switched active account to: ${res.account.name} (${res.account.model}). Note: Antigravity Agent uses Google credentials; Claude/OpenAI are active for MCP tool queries & TUI.`
      );
    } else {
      vscode.window.showInformationMessage(
        `Switched active account to: ${res.account.name} (${res.account.model})`
      );
    }
  }

  // Command: View Usage
  const viewUsageCmd = vscode.commands.registerCommand('subagent.viewUsage', async () => {
    await usageWebview.show();
  });

  // Command: Switch Active Account
  const switchAccountCmd = vscode.commands.registerCommand('subagent.switchAccount', async () => {
    const config = configManager.readConfig();
    const keychainEmail = configManager.getCurrentKeychainEmail();
    const items: (vscode.QuickPickItem & {
      accountId?: string;
      isAction?: boolean;
      action?: string;
    })[] = [];

    // Usage shortcut at top
    items.push({
      label: '$(graph) View AI Quotas & Usage Dashboard',
      description: 'Check Prompt/Flow Credits & model rate limits',
      isAction: true,
      action: 'viewUsage',
    });

    items.push({
      label: 'Configured Profiles',
      kind: vscode.QuickPickItemKind.Separator,
    });

    for (const acc of config.accounts) {
      const isActive = acc.id === config.active_account_id;
      const info = PROVIDER_INFO[acc.provider];

      let isKeychainActive = false;
      const authAny = acc.auth_type as any;
      if (authAny && typeof authAny === 'object' && authAny.oauth?.email && keychainEmail) {
        isKeychainActive = authAny.oauth.email.toLowerCase() === keychainEmail.toLowerCase();
      }

      const tags: string[] = [];
      if (isActive) tags.push('Active');
      if (isKeychainActive) tags.push('Agent Synced');
      const tagStr = tags.length > 0 ? `  (${tags.join(' & ')})` : '';

      items.push({
        label: isActive ? `$(pass-filled) ${acc.name}` : `$(${info?.icon || 'sparkle'}) ${acc.name}`,
        description: `${acc.model}${tagStr}`,
        detail: `${info?.displayName || acc.provider} • ${
          typeof acc.auth_type === 'object' ? 'Google OAuth 2.0' : 'API Key'
        }`,
        accountId: acc.id,
      });
    }

    items.push({
      label: '',
      kind: vscode.QuickPickItemKind.Separator,
    });

    items.push({
      label: '$(globe) Sign In from Web Browser (Google OAuth 2.0 / API Keys)',
      description: 'Start local auth server and open browser',
      isAction: true,
      action: 'web',
    });

    items.push({
      label: '$(add) Add Account Manually',
      description: 'Add an API key profile directly in the IDE',
      isAction: true,
      action: 'add',
    });

    items.push({
      label: '$(terminal) Open Interactive TUI Dashboard',
      description: 'Launch full terminal dashboard in Antigravity IDE',
      isAction: true,
      action: 'tui',
    });

    items.push({
      label: '$(refresh) Reload Window & Reconnect Agent',
      description: 'Restart language server to pick up new Keychain credentials',
      isAction: true,
      action: 'reload',
    });

    const selected = await vscode.window.showQuickPick(items, {
      placeHolder: 'Select an AI account to activate, or choose an action...',
      matchOnDescription: true,
      matchOnDetail: true,
    });

    if (!selected) {
      return;
    }

    if (selected.isAction) {
      switch (selected.action) {
        case 'viewUsage':
          vscode.commands.executeCommand('subagent.viewUsage');
          break;
        case 'web':
          vscode.commands.executeCommand('subagent.webSignIn');
          break;
        case 'add':
          vscode.commands.executeCommand('subagent.addAccount');
          break;
        case 'tui':
          vscode.commands.executeCommand('subagent.launchTui');
          break;
        case 'reload':
          vscode.commands.executeCommand('workbench.action.reloadWindow');
          break;
      }
      return;
    }

    if (selected.accountId) {
      const res = configManager.setActiveAccount(selected.accountId);
      await handleAccountSwitch(res);
    }
  });

  // Command: Web Sign-In
  const webSignInCmd = vscode.commands.registerCommand('subagent.webSignIn', async () => {
    const port = vscode.workspace.getConfiguration('subagent').get<number>('webAuthPort', 8085);
    const binary = configManager.getBinaryPath();

    if (!binary) {
      vscode.window.showErrorMessage(
        'Could not locate the subagent binary. Please build the project or configure "subagent.binaryPath".'
      );
      return;
    }

    if (webServerProcess && !webServerProcess.killed) {
      const url = `http://127.0.0.1:${port}`;
      vscode.env.openExternal(vscode.Uri.parse(url));
      vscode.window.showInformationMessage(`Web Sign-In portal reopened at ${url}`);
      return;
    }

    try {
      webServerProcess = cp.spawn(binary, ['web'], {
        detached: false,
        stdio: 'pipe',
      });

      const url = `http://127.0.0.1:${port}`;

      setTimeout(() => {
        vscode.env.openExternal(vscode.Uri.parse(url));
      }, 600);

      const msg = vscode.window.showInformationMessage(
        `Web Sign-In portal active at ${url}. Complete sign-in in your browser...`,
        'Re-open Browser'
      );

      msg.then((action) => {
        if (action === 'Re-open Browser') {
          vscode.env.openExternal(vscode.Uri.parse(url));
        }
      });

      webServerProcess.on('exit', () => {
        webServerProcess = null;
      });
    } catch (err) {
      vscode.window.showErrorMessage(`Failed to start Web Sign-In server: ${err}`);
    }
  });

  // Command: Launch TUI Terminal
  const launchTuiCmd = vscode.commands.registerCommand('subagent.launchTui', () => {
    const binary = configManager.getBinaryPath();
    if (!binary) {
      vscode.window.showErrorMessage(
        'Could not locate the subagent binary. Please build the project or configure "subagent.binaryPath".'
      );
      return;
    }

    const term = vscode.window.createTerminal({
      name: 'Multi-Account AI TUI',
      shellPath: binary,
    });
    term.show();
  });

  // Command: Add Account Manually
  const addAccountCmd = vscode.commands.registerCommand('subagent.addAccount', async () => {
    const providerChoices: (vscode.QuickPickItem & { provider: ProviderKind })[] = [
      {
        label: '$(sparkle) Google Gemini',
        description: 'gemini-2.5-flash, gemini-1.5-pro',
        provider: 'gemini',
      },
      {
        label: '$(hubot) Anthropic Claude',
        description: 'claude-3-5-sonnet, claude-3-5-haiku',
        provider: 'claude',
      },
      {
        label: '$(organization) OpenAI / Codex',
        description: 'gpt-4o, gpt-4o-mini, o3-mini',
        provider: 'openai',
      },
    ];

    const chosenProvider = await vscode.window.showQuickPick(providerChoices, {
      placeHolder: 'Select the AI provider...',
    });
    if (!chosenProvider) {
      return;
    }

    const provider = chosenProvider.provider;
    const providerInfo = PROVIDER_INFO[provider];

    const name = await vscode.window.showInputBox({
      title: 'Account Name / Label',
      prompt: `Enter a friendly name for this account (e.g. "Work ${providerInfo.displayName}")`,
      value: `${providerInfo.displayName} Account`,
      validateInput: (v) => (!v.trim() ? 'Name cannot be empty' : null),
    });
    if (!name) {
      return;
    }

    const apiKey = await vscode.window.showInputBox({
      title: 'API Key',
      prompt: `Enter your ${providerInfo.displayName} API Key`,
      password: true,
      validateInput: (v) => (!v.trim() ? 'API key cannot be empty' : null),
    });
    if (!apiKey) {
      return;
    }

    const modelChoices = providerInfo.models.map((m) => ({
      label: m,
      description: m === providerInfo.defaultModel ? '(Default)' : '',
    }));

    const chosenModel = await vscode.window.showQuickPick(modelChoices, {
      placeHolder: 'Select the default model for this account...',
    });
    const model = chosenModel?.label || providerInfo.defaultModel;

    const id = (await import('crypto')).randomUUID();
    const newAccount: AccountProfile = {
      id,
      name: name.trim(),
      provider,
      api_key: apiKey.trim(),
      model,
      auth_type: 'api_key',
      created_at: new Date().toISOString(),
    };

    const ok = configManager.addAccount(newAccount);
    if (ok) {
      vscode.window.showInformationMessage(
        `Added account "${newAccount.name}" (${providerInfo.displayName} / ${model}).`
      );
    } else {
      vscode.window.showErrorMessage('Failed to save account to configuration.');
    }
  });

  // Command: Delete Account
  const deleteAccountCmd = vscode.commands.registerCommand('subagent.deleteAccount', async () => {
    const config = configManager.readConfig();
    if (config.accounts.length === 0) {
      vscode.window.showInformationMessage('No accounts available to delete.');
      return;
    }

    const items = config.accounts.map((a) => ({
      label: a.name,
      description: `${PROVIDER_INFO[a.provider]?.displayName || a.provider} (${a.model})`,
      accountId: a.id,
    }));

    const chosen = await vscode.window.showQuickPick(items, {
      placeHolder: 'Select an account to remove...',
    });
    if (!chosen) {
      return;
    }

    const confirm = await vscode.window.showWarningMessage(
      `Are you sure you want to remove account "${chosen.label}"?`,
      { modal: true },
      'Delete'
    );

    if (confirm === 'Delete') {
      configManager.deleteAccount(chosen.accountId);
      vscode.window.showInformationMessage(`Removed account "${chosen.label}".`);
    }
  });

  // Command: Refresh Accounts & Usage
  const refreshAccountsCmd = vscode.commands.registerCommand('subagent.refreshAccounts', async () => {
    await usageService.refresh();
    treeProvider.refresh();
    statusBar.update();
    vscode.window.setStatusBarMessage('Multi-Account AI: Accounts and Quotas refreshed.', 2000);
  });

  // Command: Sync MCP Server
  const configureMcpCmd = vscode.commands.registerCommand('subagent.configureMcp', () => {
    mcpManager.syncMcpServer(true);
  });

  // Command: Reload Antigravity Window
  const reloadAgentCmd = vscode.commands.registerCommand('subagent.reloadAgent', () => {
    vscode.commands.executeCommand('workbench.action.reloadWindow');
  });

  // Command: Activate Account Item from Tree
  const activateAccountItemCmd = vscode.commands.registerCommand(
    'subagent.activateAccountItem',
    async (item: AccountTreeItem) => {
      if (item && item.account) {
        const res = configManager.setActiveAccount(item.account.id);
        await handleAccountSwitch(res);
      }
    }
  );

  // Command: Delete Account Item from Tree
  const deleteAccountItemCmd = vscode.commands.registerCommand(
    'subagent.deleteAccountItem',
    async (item: AccountTreeItem) => {
      if (!item || !item.account) {
        return;
      }
      const confirm = await vscode.window.showWarningMessage(
        `Are you sure you want to remove account "${item.account.name}"?`,
        { modal: true },
        'Delete'
      );
      if (confirm === 'Delete') {
        configManager.deleteAccount(item.account.id);
        vscode.window.showInformationMessage(`Removed account "${item.account.name}".`);
      }
    }
  );

  context.subscriptions.push(
    statusBar,
    treeView,
    configManager,
    usageService,
    usageWebview,
    viewUsageCmd,
    switchAccountCmd,
    webSignInCmd,
    launchTuiCmd,
    addAccountCmd,
    deleteAccountCmd,
    refreshAccountsCmd,
    configureMcpCmd,
    reloadAgentCmd,
    activateAccountItemCmd,
    deleteAccountItemCmd
  );
}

export function deactivate(): void {
  if (webServerProcess && !webServerProcess.killed) {
    try {
      webServerProcess.kill();
    } catch {
      // ignore
    }
  }
}
