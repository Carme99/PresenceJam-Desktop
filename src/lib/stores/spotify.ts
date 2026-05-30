import { writable } from 'svelte/store';

export interface SpotifyTokens {
  access_token: string;
  refresh_token: string;
  expires_at: string;
}

export interface TrackInfo {
  title: string;
  artist: string;
  album: string;
  album_art_url: string;
  is_playing: boolean;
  progress_ms: number;
  duration_ms: number;
}

export const spotifyConnected = writable(false);
export const spotifyTokens = writable<SpotifyTokens | null>(null);
