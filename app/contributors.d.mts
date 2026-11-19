export interface Contributor {
  name: string;
  login?: string;
}
export function isBot(name: string, email: string): boolean;
export function contributorsFromLog(text: string): Contributor[];
export function readContributors(cwd?: string): Contributor[];
