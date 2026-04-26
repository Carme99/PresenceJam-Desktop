import { writable } from 'svelte/store';

export type View = 'onboarding' | 'dashboard' | 'settings' | 'logs' | 'about';

export const currentView = writable<View>('dashboard');
export const isSyncing = writable(false);
export const appError = writable<string | null>(null);
