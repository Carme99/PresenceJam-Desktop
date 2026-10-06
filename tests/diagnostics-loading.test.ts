/**
 * #961 — Diagnostics' loading branch renders the shared spinner.
 *
 * A slow `get_diagnostics_snapshot` used to show the collecting label in a
 * bare `.empty-state` with no progress affordance, reading as stuck. The
 * branch now renders the shared `.spinner` above the label — the same
 * spinner-then-label shape `DeviceCodeBox` uses while waiting for sign-in.
 *
 * Fails before the fix: no `.spinner` node exists while `loading` is true.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import Diagnostics from '$lib/components/Diagnostics.svelte';

beforeEach(() => {
  invoke.mockReset();
  // Never resolves: the pane stays in its loading branch.
  const { promise } = Promise.withResolvers<unknown>();
  invoke.mockImplementation(() => promise);
});

afterEach(() => {
  cleanup();
});

describe('Diagnostics loading state (#961)', () => {
  it('renders the shared spinner above the collecting label', () => {
    const { container, getByText } = render(Diagnostics);
    const spinner = container.querySelector('.empty-state .spinner');
    expect(spinner).not.toBeNull();
    expect(spinner?.getAttribute('aria-hidden')).toBe('true');
    expect(getByText(/Collecting diagnostics/)).not.toBeNull();
  });
});
