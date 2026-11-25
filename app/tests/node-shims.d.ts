// The Ruby tests run SketchUp scripts under Node (Opal in a vm) and read fixture files; @types/node isn't a
// dependency of the app, so the few calls they use are declared here.
declare module "node:fs" {
  const fs: {
    readFileSync(p: string, enc: "utf8"): string;
    readdirSync(p: string, o: { withFileTypes: true }): { name: string; isDirectory(): boolean }[];
    existsSync(p: string): boolean;
  };
  export default fs;
}
declare module "node:path" {
  const path: { resolve(...p: string[]): string; join(...p: string[]): string; relative(a: string, b: string): string };
  export default path;
}
declare module "node:vm" {
  type Context = Record<string, unknown>;
  const vm: { createContext(o: Context): Context; runInContext(code: string, ctx: Context): unknown };
  export default vm;
  export type { Context };
}
declare const __dirname: string;
declare const process: { env: Record<string, string | undefined> };
