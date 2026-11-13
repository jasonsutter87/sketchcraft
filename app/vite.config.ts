import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // templates import ../examples from the repo root
  server: { port: 5174, strictPort: true, host: "127.0.0.1", fs: { allow: [".."] } },
});
