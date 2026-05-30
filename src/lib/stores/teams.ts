import { writable } from 'svelte/store';

export interface TeamsTokens {
  access_token: string;
  refresh_token: string;
  expires_at: string;
}

export const teamsConnected = writable(false);
export const teamsTokens = writable<TeamsTokens | null>(null);
