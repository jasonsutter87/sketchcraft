/// <reference types="vite/client" />

/** the app version from package.json (vite.config.ts) */
declare const __APP_VERSION__: string;
/** the code contributors, read from git history at build time (contributors.mjs) */
declare const __CONTRIBUTORS__: { name: string; login?: string }[];
