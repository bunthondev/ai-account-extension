import * as vscode from 'vscode';
import { UsageService, UserUsageInfo } from './usageService';

export class UsageWebviewManager {
  private panel?: vscode.WebviewPanel;
  private disposables: vscode.Disposable[] = [];

  constructor(private usageService: UsageService, private extensionUri: vscode.Uri) {
    this.usageService.onDidChangeUsage((usage) => {
      if (this.panel && usage) {
        this.panel.webview.html = this.getHtmlForWebview(this.panel.webview, usage);
      }
    });
  }

  public async show(): Promise<void> {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.Beside);
    } else {
      this.panel = vscode.window.createWebviewPanel(
        'subagentUsage',
        'AI Quotas & Usage',
        vscode.ViewColumn.Beside,
        {
          enableScripts: true,
          retainContextWhenHidden: true,
        }
      );

      this.panel.onDidDispose(
        () => {
          this.panel = undefined;
        },
        null,
        this.disposables
      );

      this.panel.webview.onDidReceiveMessage(
        async (msg) => {
          switch (msg.command) {
            case 'refresh': {
              const updated = await this.usageService.refresh();
              if (this.panel && updated) {
                this.panel.webview.html = this.getHtmlForWebview(this.panel.webview, updated);
              }
              vscode.window.setStatusBarMessage('AI Quotas refreshed.', 2000);
              break;
            }
            case 'switchAccount':
              vscode.commands.executeCommand('subagent.switchAccount');
              break;
            case 'reloadWindow':
              vscode.commands.executeCommand('workbench.action.reloadWindow');
              break;
            case 'webSignIn':
              vscode.commands.executeCommand('subagent.webSignIn');
              break;
          }
        },
        null,
        this.disposables
      );
    }

    let usage = this.usageService.getCachedUsage();
    if (!usage) {
      usage = await this.usageService.refresh();
    }

    if (usage && this.panel) {
      this.panel.webview.html = this.getHtmlForWebview(this.panel.webview, usage);
    } else if (this.panel) {
      this.panel.webview.html = this.getLoadingHtml();
    }
  }

  private getLoadingHtml(): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <style>
    body {
      font-family: var(--vscode-font-family);
      color: var(--vscode-foreground);
      background-color: var(--vscode-editor-background);
      padding: 24px;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      min-height: 200px;
    }
    .spinner {
      border: 4px solid var(--vscode-editor-lineHighlightBackground);
      border-top: 4px solid var(--vscode-progressBar-background);
      border-radius: 50%;
      width: 32px;
      height: 32px;
      animation: spin 1s linear infinite;
      margin-bottom: 16px;
    }
    @keyframes spin { 0% { transform: rotate(0deg); } 100% { transform: rotate(360deg); } }
  </style>
</head>
<body>
  <div class="spinner"></div>
  <p>Loading AI Quotas & Usage from Antigravity Language Server...</p>
</body>
</html>`;
  }

  private getHtmlForWebview(_webview: vscode.Webview, usage: UserUsageInfo): string {
    const promptPct = usage.monthlyPromptCredits > 0
      ? Math.min(100, Math.round((usage.promptCredits / usage.monthlyPromptCredits) * 100))
      : 100;
    const flowPct = usage.monthlyFlowCredits > 0
      ? Math.min(100, Math.round((usage.flowCredits / usage.monthlyFlowCredits) * 100))
      : 100;

    const modelRows = usage.models
      .map((m) => {
        const pct = Math.round(m.remainingFraction * 100);
        let barColor = 'var(--vscode-charts-green, #4caf50)';
        if (pct < 25) {
          barColor = 'var(--vscode-charts-red, #f44336)';
        } else if (pct < 60) {
          barColor = 'var(--vscode-charts-yellow, #ff9800)';
        }

        let resetFormatted = '—';
        if (m.resetTime) {
          try {
            const d = new Date(m.resetTime);
            resetFormatted = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
          } catch {
            resetFormatted = m.resetTime;
          }
        }

        return `<tr>
          <td>
            <strong>${escapeHtml(m.label)}</strong>
            ${m.tagTitle ? `<span class="badge tag">${escapeHtml(m.tagTitle)}</span>` : ''}
          </td>
          <td>
            <div class="quota-bar-container">
              <div class="quota-bar" style="width: ${pct}%; background-color: ${barColor};"></div>
            </div>
            <span class="quota-pct">${pct}%</span>
          </td>
          <td>${resetFormatted}</td>
        </tr>`;
      })
      .join('');

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>AI Quotas & Usage</title>
  <style>
    body {
      font-family: var(--vscode-font-family);
      color: var(--vscode-foreground);
      background-color: var(--vscode-editor-background);
      padding: 20px;
      line-height: 1.5;
    }
    .header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-bottom: 1px solid var(--vscode-panel-border);
      padding-bottom: 16px;
      margin-bottom: 20px;
    }
    .user-info h2 {
      margin: 0 0 4px 0;
      font-size: 1.4rem;
    }
    .user-email {
      color: var(--vscode-descriptionForeground);
      font-size: 0.95rem;
    }
    .badge {
      display: inline-block;
      padding: 3px 8px;
      border-radius: 12px;
      font-size: 0.8rem;
      font-weight: 600;
      background-color: var(--vscode-badge-background);
      color: var(--vscode-badge-foreground);
      margin-left: 8px;
    }
    .badge.pro {
      background: linear-gradient(135deg, #4285f4, #9c27b0);
      color: white;
    }
    .badge.tag {
      background-color: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
      margin-left: 6px;
      font-size: 0.75rem;
    }
    .actions {
      display: flex;
      gap: 8px;
    }
    button {
      background-color: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      padding: 6px 12px;
      border-radius: 4px;
      cursor: pointer;
      font-size: 0.85rem;
      display: flex;
      align-items: center;
      gap: 6px;
    }
    button:hover {
      background-color: var(--vscode-button-hoverBackground);
    }
    button.secondary {
      background-color: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    button.secondary:hover {
      background-color: var(--vscode-button-secondaryHoverBackground);
    }
    .cards-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px;
      margin-bottom: 24px;
    }
    .card {
      background-color: var(--vscode-sideBar-background);
      border: 1px solid var(--vscode-panel-border);
      border-radius: 8px;
      padding: 16px;
    }
    .card-title {
      font-size: 0.85rem;
      color: var(--vscode-descriptionForeground);
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-bottom: 8px;
    }
    .card-value {
      font-size: 1.8rem;
      font-weight: bold;
      margin-bottom: 6px;
    }
    .card-subtext {
      font-size: 0.85rem;
      color: var(--vscode-descriptionForeground);
      margin-bottom: 8px;
    }
    .progress-track {
      background-color: var(--vscode-editor-lineHighlightBackground);
      border-radius: 4px;
      height: 8px;
      overflow: hidden;
    }
    .progress-fill {
      height: 100%;
      border-radius: 4px;
      background: linear-gradient(90deg, #4285f4, #34a853);
    }
    .section-title {
      font-size: 1.1rem;
      font-weight: 600;
      margin: 24px 0 12px 0;
      display: flex;
      align-items: center;
      gap: 8px;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      background-color: var(--vscode-sideBar-background);
      border-radius: 8px;
      overflow: hidden;
      border: 1px solid var(--vscode-panel-border);
    }
    th, td {
      padding: 10px 14px;
      text-align: left;
      border-bottom: 1px solid var(--vscode-panel-border);
      font-size: 0.9rem;
    }
    th {
      background-color: var(--vscode-editor-lineHighlightBackground);
      font-weight: 600;
    }
    tr:last-child td {
      border-bottom: none;
    }
    .quota-bar-container {
      display: inline-block;
      width: 120px;
      background-color: var(--vscode-editor-lineHighlightBackground);
      border-radius: 4px;
      height: 8px;
      vertical-align: middle;
      overflow: hidden;
      margin-right: 8px;
    }
    .quota-bar {
      height: 100%;
      border-radius: 4px;
    }
    .quota-pct {
      font-size: 0.85rem;
      font-weight: 600;
    }
    .footer {
      margin-top: 24px;
      font-size: 0.8rem;
      color: var(--vscode-descriptionForeground);
      text-align: right;
    }
  </style>
</head>
<body>
  <div class="header">
    <div class="user-info">
      <h2>${escapeHtml(usage.name)} <span class="badge pro">${escapeHtml(usage.tierName || usage.planName)}</span></h2>
      <div class="user-email">${escapeHtml(usage.email)}</div>
    </div>
    <div class="actions">
      <button onclick="post('refresh')">🔄 Refresh</button>
      <button class="secondary" onclick="post('switchAccount')">🔀 Switch Account</button>
      <button class="secondary" onclick="post('reloadWindow')">⚡ Reload Agent</button>
    </div>
  </div>

  <div class="cards-grid">
    <div class="card">
      <div class="card-title">Available Prompt Credits</div>
      <div class="card-value">${usage.promptCredits.toLocaleString()}</div>
      <div class="card-subtext">${usage.monthlyPromptCredits.toLocaleString()} monthly pool</div>
      <div class="progress-track">
        <div class="progress-fill" style="width: ${promptPct}%;"></div>
      </div>
    </div>
    <div class="card">
      <div class="card-title">Available Flow Credits</div>
      <div class="card-value">${usage.flowCredits.toLocaleString()}</div>
      <div class="card-subtext">${usage.monthlyFlowCredits.toLocaleString()} monthly pool</div>
      <div class="progress-track">
        <div class="progress-fill" style="width: ${flowPct}%;"></div>
      </div>
    </div>
    <div class="card">
      <div class="card-title">Active Antigravity Plan</div>
      <div class="card-value" style="font-size: 1.4rem; padding-top: 4px;">${escapeHtml(usage.planName)}</div>
      <div class="card-subtext">${escapeHtml(usage.tierName)}</div>
      <div style="font-size: 0.85rem; color: var(--vscode-charts-green); font-weight: 600; margin-top: 10px;">
        ● All Model Quotas Active
      </div>
    </div>
  </div>

  <div class="section-title">
    <span>Model Rate Limits & Remaining Quotas</span>
  </div>

  <table>
    <thead>
      <tr>
        <th>Model Name</th>
        <th>Quota Remaining</th>
        <th>Reset Time</th>
      </tr>
    </thead>
    <tbody>
      ${modelRows}
    </tbody>
  </table>

  <div class="footer">
    Last updated: ${usage.fetchedAt.toLocaleTimeString()} • Powered by Antigravity Language Server
  </div>

  <script>
    const vscode = acquireVsCodeApi();
    function post(command) {
      vscode.postMessage({ command });
    }
  </script>
</body>
</html>`;
  }

  public dispose(): void {
    if (this.panel) {
      this.panel.dispose();
    }
    this.disposables.forEach((d) => d.dispose());
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
