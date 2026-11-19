// The "Thank you" list in Help > About: everyone who has authored a commit, read from git history when the app is
// built (vite.config.ts bakes it in as __CONTRIBUTORS__). Nothing here is kept by hand.
//
// - Authors only: "Co-authored-by" lines are ignored.
// - Bots (dependabot, github-actions, anything ending in [bot]) are left out.
// - One person once: commits are merged by email, by GitHub login (from a users.noreply.github.com address) and by
//   name, so a change of laptop or a web-UI commit doesn't add a second line. .mailmap is honored.
// - Order: by first contribution, so the person who started the project comes first and the rest follow as they joined.
// - No git, no .git folder, or a shallow clone that can't be deepened: an empty list, never an error. (A shallow
//   history would list only the latest authors, which is wrong, so it is treated as no history.)
import { execFileSync } from "node:child_process";

const BOT = /\[bot\]|^(dependabot|github-actions|renovate|web-flow)\b|-bot$|^bot$/i;
const NOREPLY = /^(?:\d+\+)?([A-Za-z0-9-]+)@users\.noreply\.github\.com$/i;

/** Is this author a bot, or GitHub itself? */
export function isBot(name, email) {
  return BOT.test(name.trim()) || BOT.test(email.split("@")[0]) || /^noreply@github\.com$/i.test(email.trim());
}

/**
 * Contributors from `git log --reverse --format=%aN%x09%aE` output (oldest first), as [{ name, login? }].
 * `login` is the GitHub login when the commit used a GitHub no-reply address.
 */
export function contributorsFromLog(text) {
  const people = [];
  const byKey = new Map();
  for (const line of String(text ?? "").split(/\r?\n/)) {
    const tab = line.indexOf("\t");
    if (tab < 0) continue;
    const name = line.slice(0, tab).replace(/\s+/g, " ").trim();
    const rawEmail = line.slice(tab + 1).trim();
    const email = rawEmail.toLowerCase();
    if (!name && !email) continue;
    if (isBot(name, email)) continue;
    const login = NOREPLY.exec(rawEmail)?.[1];
    const keys = [email && `e:${email}`, login && `l:${login.toLowerCase()}`, name && `n:${name.toLowerCase()}`].filter(Boolean);
    const known = keys.map((k) => byKey.get(k)).find(Boolean);
    const person = known ?? { name: name || login, ...(login ? { login } : {}) };
    if (!known) people.push(person);
    else if (login && !person.login) person.login = login;
    for (const k of keys) byKey.set(k, person);
  }
  return people;
}

const git = (args, cwd) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], timeout: 60_000, maxBuffer: 64 * 1024 * 1024 });

/** The contributors of the repository at `cwd` ([] when there is no usable history). */
export function readContributors(cwd = process.cwd()) {
  try {
    if (git(["rev-parse", "--is-shallow-repository"], cwd).trim() === "true") {
      // CI checks out one commit by default; fetch the rest (no credentials needed for a public repo).
      // SKETCHCRAFT_NO_UNSHALLOW=1 skips this.
      if (process.env.SKETCHCRAFT_NO_UNSHALLOW) return [];
      git(["fetch", "--quiet", "--unshallow"], cwd);
      if (git(["rev-parse", "--is-shallow-repository"], cwd).trim() === "true") return [];
    }
    return contributorsFromLog(git(["log", "--reverse", "--format=%aN%x09%aE"], cwd));
  } catch {
    return [];
  }
}
