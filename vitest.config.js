import { defineConfig } from 'vitest/config';

// #443: minimal frontend harness — node environment only, no DOM
// plugins, no app restructure. Tests import pure logic (i18n dicts,
// store helpers) and assert observable behavior. ONE npm script:
// `npm test` -> `vitest run`.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts']
  }
});
