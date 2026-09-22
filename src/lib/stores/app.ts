import { writable } from 'svelte/store';

export type View = 'onboarding' | 'dashboard' | 'settings' | 'logs' | 'about' | 'reconnect' | 'diagnostics';

export const currentView = writable<View>('dashboard');
export const appError = writable<string | null>(null);
// #817: Settings publishes its draft state here so the root page's `navigate`
// listener can park a menu navigation while edits are unsaved (instead of
// unmounting the form and destroying the draft). `pendingMenuNav` holds the
// parked target until the banner's Save / Discard / Stay choice resolves it.
export const settingsDirty = writable<boolean>(false);
export const pendingMenuNav = writable<View | null>(null);
