import { writable } from 'svelte/store';

export interface TeamsTokens {
  access_token: string;
  refresh_token: string;
  expires_at: string;
}

