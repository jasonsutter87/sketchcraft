// Unit tests for the interface's pure code (npm test). The browser suites live in tools/ui-tests.
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.{ts,mjs}"],
    environment: "node",
    // a unit test that needs a browser, a network or the real home folder is in the wrong place
    testTimeout: 10_000,
  },
});
