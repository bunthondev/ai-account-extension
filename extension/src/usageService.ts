import * as http from 'http';
import * as cp from 'child_process';
import * as vscode from 'vscode';

export interface ModelQuota {
  label: string;
  modelId: string;
  remainingFraction: number; // 0.0 to 1.0
  resetTime?: string;
  tagTitle?: string;
  tagDescription?: string;
}

export interface UserUsageInfo {
  name: string;
  email: string;
  planName: string;
  tierName: string;
  promptCredits: number;
  monthlyPromptCredits: number;
  flowCredits: number;
  monthlyFlowCredits: number;
  models: ModelQuota[];
  fetchedAt: Date;
}

export class UsageService {
  private cachedUsage: UserUsageInfo | null = null;
  private changeEmitter = new vscode.EventEmitter<UserUsageInfo | null>();
  public readonly onDidChangeUsage = this.changeEmitter.event;
  private pollInterval?: NodeJS.Timeout;

  constructor() {
    this.refresh();
    // Refresh usage every 60 seconds
    this.pollInterval = setInterval(() => {
      this.refresh();
    }, 60000);
  }

  public getCachedUsage(): UserUsageInfo | null {
    return this.cachedUsage;
  }

  public async refresh(): Promise<UserUsageInfo | null> {
    const usage = await this.fetchUsage();
    if (usage) {
      this.cachedUsage = usage;
      this.changeEmitter.fire(usage);
    }
    return usage;
  }

  public async fetchUsage(): Promise<UserUsageInfo | null> {
    const connectionInfo = this.resolveConnection();
    if (!connectionInfo) {
      return null;
    }

    const { host, port, csrfToken } = connectionInfo;

    return new Promise((resolve) => {
      const postData = '{}';
      const req = http.request(
        {
          host,
          port,
          path: '/exa.language_server_pb.LanguageServerService/GetUserStatus',
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'x-codeium-csrf-token': csrfToken,
            'Content-Length': Buffer.byteLength(postData),
          },
          timeout: 4000,
        },
        (res) => {
          let body = '';
          res.on('data', (chunk) => {
            body += chunk;
          });
          res.on('end', () => {
            try {
              if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
                const parsed = JSON.parse(body);
                const status = parsed.userStatus;
                if (!status) {
                  resolve(null);
                  return;
                }

                const planInfo = status.planStatus?.planInfo || {};
                const models: ModelQuota[] = [];

                const clientConfigs = status.cascadeModelConfigData?.clientModelConfigs || [];
                for (const cfg of clientConfigs) {
                  if (cfg.label) {
                    models.push({
                      label: cfg.label,
                      modelId: cfg.modelId || cfg.label,
                      remainingFraction:
                        typeof cfg.quotaInfo?.remainingFraction === 'number'
                          ? cfg.quotaInfo.remainingFraction
                          : 1.0,
                      resetTime: cfg.quotaInfo?.resetTime,
                      tagTitle: cfg.tagTitle,
                      tagDescription: cfg.tagDescription,
                    });
                  }
                }

                const usage: UserUsageInfo = {
                  name: status.name || 'User',
                  email: status.email || '',
                  planName: planInfo.planName || 'Standard',
                  tierName: status.userTier?.name || 'Google AI',
                  promptCredits: status.planStatus?.availablePromptCredits ?? 0,
                  monthlyPromptCredits: planInfo.monthlyPromptCredits ?? 0,
                  flowCredits: status.planStatus?.availableFlowCredits ?? 0,
                  monthlyFlowCredits: planInfo.monthlyFlowCredits ?? 0,
                  models,
                  fetchedAt: new Date(),
                };

                resolve(usage);
                return;
              }
            } catch (err) {
              console.warn('Failed to parse GetUserStatus response:', err);
            }
            resolve(null);
          });
        }
      );

      req.on('error', (err) => {
        console.warn('GetUserStatus request error:', err.message);
        resolve(null);
      });

      req.on('timeout', () => {
        req.destroy();
        resolve(null);
      });

      req.write(postData);
      req.end();
    });
  }

  private resolveConnection(): { host: string; port: number; csrfToken: string } | null {
    let host = 'localhost';
    let port = 5387;
    let csrfToken = process.env.ANTIGRAVITY_CSRF_TOKEN || '';

    if (process.env.ANTIGRAVITY_LS_ADDRESS) {
      const parts = process.env.ANTIGRAVITY_LS_ADDRESS.split(':');
      host = parts[0] || 'localhost';
      port = parseInt(parts[1] || '5387', 10);
    } else {
      // Try to find --hub-port from running agy process
      try {
        const ps = cp.execSync("ps aux | grep 'agy --hub' | grep -v grep", {
          encoding: 'utf-8',
        });
        const match = ps.match(/--hub-port=(\d+)/);
        if (match) {
          port = parseInt(match[1], 10);
        }
      } catch {
        // fallback to default
      }
    }

    return { host, port, csrfToken };
  }

  public dispose(): void {
    if (this.pollInterval) {
      clearInterval(this.pollInterval);
    }
    this.changeEmitter.dispose();
  }
}
