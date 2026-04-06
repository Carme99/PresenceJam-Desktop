import { writable } from 'svelte/store';
import type { TrackInfo } from './spotify';

export type View = 'onboarding' | 'dashboard' | 'settings' | 'logs';

export const currentView = writable<View>('dashboard');
export const isSyncing = writable(false);
export const appError = writable<string | null>(null);
export const currentTrack = writable<TrackInfo | null>(null);
