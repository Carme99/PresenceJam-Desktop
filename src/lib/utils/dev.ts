/**
 * Dev-mode flag for conditional debug logging.
 * In SvelteKit, import.meta.env.DEV is true during development (npm run dev)
 * and false in production builds.
 */
export const isDev: boolean = import.meta.env.DEV;

/**
 * Conditional debug logger — only logs when running in development mode.
 * Use instead of console.log for verbose/debug output that should not appear
 * in production console output.
 *
 * Usage:
 *   import { devLog } from '$lib/utils/dev';
 *   devLog('[ONBOARDING] connectSpotify: entry');
 */
export function devLog(...args: unknown[]): void {
  if (isDev) {
    console.log(...args);
  }
}
