import * as vscode from 'vscode';
import { ConfigManager } from './configManager';
import { UsageService, UserUsageInfo } from './usageService';
import { PROVIDER_INFO } from './types';

export class StatusBarManager {
  private accountStatusBarItem: vscode.StatusBarItem;
  private quotaStatusBarItem: vscode.StatusBarItem;
  private configManager: ConfigManager;
  private usageService: UsageService;

  constructor(configManager: ConfigManager, usageService: UsageService) {
    this.configManager = configManager;
    this.usageService = usageService;

    // Account item
    this.accountStatusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      100
    );
    this.accountStatusBarItem.command = 'subagent.switchAccount';

    // Quota / Usage item
    this.quotaStatusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      99
    );
    this.quotaStatusBarItem.command = 'subagent.viewUsage';

    this.update();

    configManager.onDidChangeConfig(() => {
      this.update();
    });

    usageService.onDidChangeUsage((usage) => {
      this.updateQuota(usage);
    });

    vscode.workspace.onDidChangeConfiguration((e) => {
      if (
        e.affectsConfiguration('subagent.showStatusBar') ||
        e.affectsConfiguration('subagent.showQuotaStatusBar')
      ) {
        this.update();
      }
    });
  }

  public update(): void {
    const showAccount = vscode.workspace
      .getConfiguration('subagent')
      .get<boolean>('showStatusBar', true);
    const showQuota = vscode.workspace
      .getConfiguration('subagent')
      .get<boolean>('showQuotaStatusBar', true);

    if (!showAccount) {
      this.accountStatusBarItem.hide();
    } else {
      const active = this.configManager.getActiveAccount();
      if (!active) {
        this.accountStatusBarItem.text = `$(organization) AI: No Accounts`;
        this.accountStatusBarItem.tooltip =
          'Multi-Account AI: Click to add or configure AI accounts';
        this.accountStatusBarItem.show();
      } else {
        const providerInfo = PROVIDER_INFO[active.provider] || {
          displayName: active.provider,
          icon: 'sparkle',
        };

        const icon = providerInfo.icon;
        this.accountStatusBarItem.text = `$(${icon}) ${active.name} (${active.model})`;

        const tooltip = new vscode.MarkdownString();
        tooltip.isTrusted = true;
        tooltip.appendMarkdown(`### Active AI Account: **${active.name}**\n\n`);
        tooltip.appendMarkdown(`- **Provider:** ${providerInfo.displayName}\n`);
        tooltip.appendMarkdown(`- **Model:** \`${active.model}\`\n`);
        tooltip.appendMarkdown(
          `- **Auth Type:** ${
            typeof active.auth_type === 'object' ? 'Google OAuth 2.0' : 'API Key'
          }\n\n`
        );
        tooltip.appendMarkdown(`---\n\n`);
        tooltip.appendMarkdown(`[$(graph) View Quotas & Usage](command:subagent.viewUsage) | `);
        tooltip.appendMarkdown(`[$(arrow-swap) Switch Account](command:subagent.switchAccount) | `);
        tooltip.appendMarkdown(`[$(globe) Web Sign-In](command:subagent.webSignIn) | `);
        tooltip.appendMarkdown(`[$(terminal) TUI](command:subagent.launchTui)`);

        this.accountStatusBarItem.tooltip = tooltip;
        this.accountStatusBarItem.show();
      }
    }

    if (!showQuota) {
      this.quotaStatusBarItem.hide();
    } else {
      this.updateQuota(this.usageService.getCachedUsage());
    }
  }

  private updateQuota(usage: UserUsageInfo | null): void {
    const showQuota = vscode.workspace
      .getConfiguration('subagent')
      .get<boolean>('showQuotaStatusBar', true);
    if (!showQuota) {
      this.quotaStatusBarItem.hide();
      return;
    }

    if (!usage) {
      this.quotaStatusBarItem.text = `$(graph) Quota: ...`;
      this.quotaStatusBarItem.tooltip = 'Click to load AI Quotas & Usage';
      this.quotaStatusBarItem.show();
      return;
    }

    const minQuotaFraction = usage.models.reduce((min, m) => {
      return m.remainingFraction < min ? m.remainingFraction : min;
    }, 1.0);

    const minQuotaPct = Math.round(minQuotaFraction * 100);
    this.quotaStatusBarItem.text = `$(graph) Quota: ${minQuotaPct}% (${usage.promptCredits} cr)`;

    const tooltip = new vscode.MarkdownString();
    tooltip.isTrusted = true;
    tooltip.appendMarkdown(`### AI Quotas & Credits (${usage.email})\n\n`);
    tooltip.appendMarkdown(`- **Plan:** ${usage.tierName || usage.planName}\n`);
    tooltip.appendMarkdown(
      `- **Prompt Credits:** ${usage.promptCredits.toLocaleString()} / ${usage.monthlyPromptCredits.toLocaleString()}\n`
    );
    tooltip.appendMarkdown(
      `- **Flow Credits:** ${usage.flowCredits.toLocaleString()} / ${usage.monthlyFlowCredits.toLocaleString()}\n`
    );
    tooltip.appendMarkdown(`- **Active Models:** ${usage.models.length} registered\n\n`);
    tooltip.appendMarkdown(`---\n\n`);
    tooltip.appendMarkdown(`[$(graph) Open Full Quotas & Usage Dashboard](command:subagent.viewUsage)`);

    this.quotaStatusBarItem.tooltip = tooltip;
    this.quotaStatusBarItem.show();
  }

  public dispose(): void {
    this.accountStatusBarItem.dispose();
    this.quotaStatusBarItem.dispose();
  }
}
