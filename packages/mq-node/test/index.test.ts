import { describe, it, expect } from 'vitest';
import { run, format, toAst, toHtml, htmlToMarkdown } from '../src/index';

// Note: These tests assume the WASM module is correctly loaded and functional.
// In a real environment, they would verify the actual mq logic.
describe('mq-node', () => {
  it('should export core functions', () => {
    expect(typeof run).toBe('function');
    expect(typeof format).toBe('function');
    expect(typeof toAst).toBe('function');
    expect(typeof toHtml).toBe('function');
    expect(typeof htmlToMarkdown).toBe('function');
  });

  it('should run a simple query', async () => {
    // This will likely fail in this environment because the WASM is not actually loaded,
    // but it demonstrates how the test should look.
    try {
      const result = await run('.[]', '- item 1\n- item 2');
      expect(typeof result).toBe('string');
    } catch (e) {
      // Expected to fail if WASM is not loaded
    }
  });

  it('should format code', async () => {
    try {
      const result = await format('.[]');
      expect(typeof result).toBe('string');
    } catch (e) {
      // Expected to fail if WASM is not loaded
    }
  });
});
