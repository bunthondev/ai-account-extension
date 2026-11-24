export type ProviderKind = 'gemini' | 'claude' | 'openai';

export interface OAuthDetails {
  refresh_token?: string;
  client_id?: string;
  client_secret?: string;
  expires_at?: string;
  email?: string;
}

export type AuthType = 'api_key' | 'oauth' | { oauth: OAuthDetails };

export interface AccountProfile {
  id: string;
  name: string;
  provider: ProviderKind;
  api_key: string;
  model: string;
  base_url?: string | null;
  auth_type: AuthType;
  created_at?: string;
}

export interface AppConfig {
  active_account_id: string | null;
  accounts: AccountProfile[];
}

export const PROVIDER_INFO: Record<
  ProviderKind,
  { displayName: string; defaultModel: string; models: string[]; icon: string }
> = {
  gemini: {
    displayName: 'Google Gemini',
    defaultModel: 'gemini-2.5-flash',
    models: ['gemini-2.5-flash', 'gemini-1.5-pro', 'gemini-2.0-flash-exp'],
    icon: 'sparkle',
  },
  claude: {
    displayName: 'Anthropic Claude',
    defaultModel: 'claude-3-5-sonnet-20241022',
    models: ['claude-3-5-sonnet-20241022', 'claude-3-5-haiku-20241022', 'claude-3-opus-20240229'],
    icon: 'hubot',
  },
  openai: {
    displayName: 'OpenAI / Codex',
    defaultModel: 'gpt-4o',
    models: ['gpt-4o', 'gpt-4o-mini', 'o3-mini', 'o1'],
    icon: 'organization',
  },
};
