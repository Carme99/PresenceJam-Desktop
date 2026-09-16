/**
 * #530 — boot routing.
 *
 * A returning user whose stored session needs a sign-in must not be sent to
 * the first-run wizard (which asks for the Client ID/Secret again and, on
 * completion, overwrites the stored settings). The complete verdict keeps the
 * existing dashboard/onboarding split; the incomplete-but-configured case is
 * the one this slice adds.
 */
import { describe, it, expect } from 'vitest';
import { bootView } from '$lib/utils/boot';

describe('bootView', () => {
  it('lands a complete install on the dashboard', () => {
    expect(bootView(true, true)).toBe('dashboard');
    expect(bootView(true, false)).toBe('dashboard');
  });

  it('sends a returning user with stored credentials to reconnect, not the wizard', () => {
    expect(bootView(false, true)).toBe('reconnect');
  });

  it('keeps the setup wizard for an install with no stored credentials', () => {
    expect(bootView(false, false)).toBe('onboarding');
  });
});
