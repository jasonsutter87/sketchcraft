// Unit tests for the interface's pure code (npm test). The browser suites live in tools/ui-tests.
import { defineConfig } from "vitest/config";

// Dead-code probes (tools/tombstones): only in a throwaway tombstone checkout, where SKETCHCRAFT_TOMB_LOG is set.
const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const tombstones = env.SKETCHCRAFT_TOMB_LOG ? [(await import("../tools/tombstones/vite-plugin.mjs")).default()] : [];

export default defineConfig({
  plugins: tombstones,
  test: {
    include: ["tests/**/*.test.{ts,mjs}"],
    environment: "node",
    // a unit test that needs a browser, a network or the real home folder is in the wrong place
    testTimeout: 10_000,
  },
});
