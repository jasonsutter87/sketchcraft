import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import pkg from "./package.json";
import { readContributors } from "./contributors.mjs";
import bridgeToken,{ devCsp } from "./vite-bridge-token.mjs";

// The dev page's port (default 5174). Other values are for throwaway test
// instances (docs/dev/building.md); the bridge defaults to 5175.
const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const PORT = Number(env.SKETCHCRAFT_DEV_PORT ?? 5174);
const BRIDGE_PORT = Number(env.VITE_BRIDGE_PORT ?? 5175);

// Dead-code probes (tools/tombstones, docs/dev/tombstones.md): loaded only when SKETCHCRAFT_TOMB_LOG is set,
// which only happens in a throwaway tombstone checkout. A normal run never touches that file.
const tombstones = env.SKETCHCRAFT_TOMB_LOG ? [(await import("../tools/tombstones/vite-plugin.mjs")).default()] : [];

export default defineConfig({
  plugins: [react(), bridgeToken(PORT), devCsp(PORT, BRIDGE_PORT), ...tombstones],
  // the app's version, shown in the File menu (kept equal to tauri.conf.json's)
  // and the people thanked in Help > About, read from git history (contributors.mjs; [] without history)
  define: { __APP_VERSION__: JSON.stringify(pkg.version), __CONTRIBUTORS__: JSON.stringify(readContributors()) },
  clearScreen: false,
  // templates import ../examples from the repo root
  server: { port: PORT, strictPort: true, host: "127.0.0.1", fs: { allow: [".."] } },
});
