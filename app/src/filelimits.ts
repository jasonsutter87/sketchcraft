/** The biggest file the browser build reads into memory (checked before reading). */
export const MAX_BROWSER_FILE = 256 * 1024 * 1024;
export const fileTooBig = (f: { size: number }) => f.size > MAX_BROWSER_FILE;
