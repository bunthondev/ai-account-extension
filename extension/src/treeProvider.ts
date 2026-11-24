import * as vscode from 'vscode';
import { ConfigManager } from './configManager';
import { UsageService, UserUsageInfo, ModelQuota } from './usageService';
import { AccountProfile, ProviderKind, PROVIDER_INFO } from './types';

export class AccountTreeItem extends vscode.TreeItem {
  constructor(
    public readonly account: AccountProfile,
    public readonly isActive: boolean,
    public readonly isAgentKeychainActive: boolean,
    public readonly collapsibleState: vscode.TreeItemCollapsibleState = vscode.TreeItemCollapsibleState.None
  ) {
    super(account.name, collapsibleState);

    const providerInfo = PROVIDER_INFO[account.provider];
    const tags: string[] = [];
    if (isActive) tags.push('Active');
    if (isAgentKeychainActive) tags.push('Agent Synced');

    const tagStr = tags.length > 0 ? `  ● ${tags.join(' & ')}` : '';
    this.description = `${account.model}${tagStr}`;
    this.contextValue = isActive ? 'account-active' : 'account-inactive';

    if (isActive && isAgentKeychainActive) {
      this.iconPath = new vscode.ThemeIcon('pass-filled', new vscode.ThemeColor('charts.green'));
    } else if (isActive) {
      this.iconPath = new vscode.ThemeIcon('check', new vscode.ThemeColor('charts.blue'));
    } else if (isAgentKeychainActive) {
      this.iconPath = new vscode.ThemeIcon('key', new vscode.ThemeColor('charts.yellow'));
    } else {
      this.iconPath = new vscode.ThemeIcon(providerInfo?.icon || 'sparkle');
    }

    const md = new vscode.MarkdownString();
    md.appendMarkdown(`### **${account.name}**\n\n`);
    md.appendMarkdown(`- **Subagent Active:** ${isActive ? '✅ Yes' : 'No'}\n`);
    md.appendMarkdown(
      `- **Antigravity Agent Keychain:** ${isAgentKeychainActive ? '🔑 Active' : 'Inactive'}\n`
    );
    md.appendMarkdown(`- **Provider:** ${providerInfo?.displayName || account.provider}\n`);
    md.appendMarkdown(`- **Model:** \`${account.model}\`\n`);

    let email = '';
    const authAny = account.auth_type as any;
    if (authAny && typeof authAny === 'object' && authAny.oauth?.email) {
      email = authAny.oauth.email;
    }
    if (email) {
      md.appendMarkdown(`- **Email:** \`${email}\`\n`);
    }

    if (account.api_key) {
      const masked =
        account.api_key.length > 8
          ? `${account.api_key.substring(0, 4)}...${account.api_key.substring(
              account.api_key.length - 4
            )}`
          : '****';
      md.appendMarkdown(`- **Key / Auth:** \`${masked}\`\n`);
    }
    md.appendMarkdown(`\n---\n[$(graph) Click to View AI Quotas & Usage](command:subagent.viewUsage)`);
    this.tooltip = md;

    if (!isActive) {
      this.command = {
        command: 'subagent.activateAccountItem',
        title: 'Activate Account',
        arguments: [this],
      };
    }
  }
}

export class ProviderCategoryTreeItem extends vscode.TreeItem {
  constructor(
    public readonly provider: ProviderKind,
    public readonly count: number,
    public readonly hasActive: boolean
  ) {
    const info = PROVIDER_INFO[provider];
    super(info.displayName, vscode.TreeItemCollapsibleState.Expanded);
    this.description = `${count} ${count === 1 ? 'account' : 'accounts'}${
      hasActive ? ' (Active)' : ''
    }`;
    this.iconPath = new vscode.ThemeIcon(info.icon);
    this.contextValue = 'provider-category';
  }
}

export class UsageCategoryTreeItem extends vscode.TreeItem {
  constructor(public readonly usage: UserUsageInfo | null) {
    super('AI Quotas & Credits', vscode.TreeItemCollapsibleState.Expanded);
    if (usage) {
      this.description = `${usage.email} • ${usage.promptCredits} Prompt / ${usage.flowCredits} Flow`;
    } else {
      this.description = 'Click to load...';
    }
    this.iconPath = new vscode.ThemeIcon('graph', new vscode.ThemeColor('charts.purple'));
    this.contextValue = 'usage-category';
    this.command = {
      command: 'subagent.viewUsage',
      title: 'View AI Quotas & Usage',
    };
  }
}

export class UsageDetailTreeItem extends vscode.TreeItem {
  constructor(label: string, description: string, icon: string, tooltipText?: string) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = description;
    this.iconPath = new vscode.ThemeIcon(icon);
    if (tooltipText) {
      this.tooltip = tooltipText;
    }
    this.command = {
      command: 'subagent.viewUsage',
      title: 'View AI Quotas & Usage',
    };
  }
}

type TreeNode =
  | ProviderCategoryTreeItem
  | AccountTreeItem
  | UsageCategoryTreeItem
  | UsageDetailTreeItem;

export class AccountsTreeDataProvider implements vscode.TreeDataProvider<TreeNode> {
  private _onDidChangeTreeData: vscode.EventEmitter<TreeNode | undefined | void> =
    new vscode.EventEmitter<TreeNode | undefined | void>();
  readonly onDidChangeTreeData: vscode.Event<TreeNode | undefined | void> =
    this._onDidChangeTreeData.event;

  constructor(
    private configManager: ConfigManager,
    private usageService: UsageService
  ) {
    configManager.onDidChangeConfig(() => {
      this.refresh();
    });
    usageService.onDidChangeUsage(() => {
      this.refresh();
    });
  }

  public refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  public getTreeItem(element: TreeNode): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: TreeNode): Thenable<TreeNode[]> {
    const config = this.configManager.readConfig();
    const accounts = config.accounts;
    const keychainEmail = this.configManager.getCurrentKeychainEmail();
    const usage = this.usageService.getCachedUsage();

    if (!element) {
      const roots: TreeNode[] = [];

      // 1. Quota & Usage Root Category
      roots.push(new UsageCategoryTreeItem(usage));

      // 2. Providers
      const providers: ProviderKind[] = ['gemini', 'claude', 'openai'];
      for (const p of providers) {
        const matching = accounts.filter((a) => a.provider === p);
        if (matching.length > 0) {
          const hasActive = matching.some((a) => a.id === config.active_account_id);
          roots.push(new ProviderCategoryTreeItem(p, matching.length, hasActive));
        }
      }

      return Promise.resolve(roots);
    }

    if (element instanceof UsageCategoryTreeItem) {
      const items: UsageDetailTreeItem[] = [];
      if (!usage) {
        items.push(
          new UsageDetailTreeItem(
            'Click to load quota details',
            'Connects to Antigravity Language Server',
            'sync'
          )
        );
        return Promise.resolve(items);
      }

      items.push(
        new UsageDetailTreeItem(
          'Plan',
          `${usage.planName} (${usage.tierName})`,
          'shield',
          'Current active Antigravity subscription tier'
        )
      );
      items.push(
        new UsageDetailTreeItem(
          'Prompt Credits',
          `${usage.promptCredits.toLocaleString()} available (${usage.monthlyPromptCredits.toLocaleString()} monthly)`,
          'pulse',
          'Available prompt credits for active reasoning'
        )
      );
      items.push(
        new UsageDetailTreeItem(
          'Flow Credits',
          `${usage.flowCredits.toLocaleString()} available (${usage.monthlyFlowCredits.toLocaleString()} monthly)`,
          'zap',
          'Available flow credits for agentic execution'
        )
      );

      // Add prominent models
      const topModels = usage.models.filter(
        (m) =>
          m.label.includes('Gemini 4') ||
          m.label.includes('Gemini 3.8') ||
          m.label.includes('Claude Sonnet') ||
          m.label.includes('Claude Opus')
      ).slice(0, 4);

      for (const m of topModels) {
        const pct = Math.round(m.remainingFraction * 100);
        items.push(
          new UsageDetailTreeItem(
            m.label,
            `${pct}% remaining`,
            pct < 25 ? 'error' : pct < 60 ? 'warning' : 'pass',
            `Quota remaining: ${pct}%. Click to view full details.`
          )
        );
      }

      items.push(
        new UsageDetailTreeItem(
          'Open Usage Dashboard',
          'View all models & countdown timers →',
          'arrow-right',
          'Click to open the full interactive dashboard'
        )
      );

      return Promise.resolve(items);
    }

    if (element instanceof ProviderCategoryTreeItem) {
      const matching = accounts.filter((a) => a.provider === element.provider);
      const items = matching.map((acc) => {
        const isActive = acc.id === config.active_account_id;
        let isAgentKeychainActive = false;

        const authAny = acc.auth_type as any;
        let accEmail = '';
        if (authAny && typeof authAny === 'object' && authAny.oauth?.email) {
          accEmail = authAny.oauth.email;
        } else if (acc.name.includes('@')) {
          const m = acc.name.match(/[\w.-]+@[\w.-]+\.\w+/);
          if (m) accEmail = m[0];
        }

        if (keychainEmail && accEmail && keychainEmail.toLowerCase() === accEmail.toLowerCase()) {
          isAgentKeychainActive = true;
        }

        return new AccountTreeItem(acc, isActive, isAgentKeychainActive);
      });
      return Promise.resolve(items);
    }

    return Promise.resolve([]);
  }
}
