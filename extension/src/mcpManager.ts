import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as vscode from 'vscode';
import { ConfigManager } from './configManager';

export class McpManager {
  constructor(private configManager: ConfigManager) {}

  public syncMcpServer(notify = false): boolean {
    const autoSync = vscode.workspace
      .getConfiguration('subagent')
      .get<boolean>('autoSyncMcp', true);

    if (!autoSync && !notify) {
      return false;
    }

    const binaryPath = this.configManager.getBinaryPath();
    if (!binaryPath) {
      if (notify) {
        vscode.window.showWarningMessage(
          'Could not find subagent binary. Please compile the Rust project or set "subagent.binaryPath" in settings.'
        );
      }
      return false;
    }

    const candidateMcpFiles = [
      path.join(
        os.homedir(),
        'Library',
        'Application Support',
        'Antigravity IDE',
        'User',
        'mcp.json'
      ),
      path.join(os.homedir(), '.antigravity-ide', 'User', 'mcp.json'),
    ];

    let updated = false;

    for (const mcpFile of candidateMcpFiles) {
      const parentDir = path.dirname(mcpFile);
      if (fs.existsSync(parentDir)) {
        try {
          let mcpData: { servers?: Record<string, { command: string; args: string[] }>; inputs?: any[] } = {
            servers: {},
            inputs: [],
          };

          if (fs.existsSync(mcpFile)) {
            const raw = fs.readFileSync(mcpFile, 'utf-8');
            mcpData = JSON.parse(raw);
            if (!mcpData.servers) {
              mcpData.servers = {};
            }
          }

          const current = mcpData.servers?.['multi-account-ai'];
          if (
            !current ||
            current.command !== binaryPath ||
            !Array.isArray(current.args) ||
            !current.args.includes('--mcp')
          ) {
            mcpData.servers = mcpData.servers || {};
            mcpData.servers['multi-account-ai'] = {
              command: binaryPath,
              args: ['--mcp'],
            };
            fs.writeFileSync(mcpFile, JSON.stringify(mcpData, null, '\t') + '\n', 'utf-8');
            updated = true;
          }
        } catch (err) {
          console.error(`Failed to update MCP config at ${mcpFile}:`, err);
        }
      }
    }

    if (notify) {
      if (updated) {
        vscode.window.showInformationMessage(
          `Registered "multi-account-ai" MCP server in Antigravity IDE with binary: ${binaryPath}`
        );
      } else {
        vscode.window.showInformationMessage(
          `Antigravity IDE MCP server "multi-account-ai" is already up to date.`
        );
      }
    }

    return updated;
  }
}
