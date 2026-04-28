import { writable } from 'svelte/store';

export type View = 'onboarding' | 'dashboard' | 'settings' | 'logs' | 'about' | 'reconnect';

export const currentView = writable<View>('dashboard');
export const appError = writable<string | null>(null);
