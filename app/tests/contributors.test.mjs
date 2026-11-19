import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { contributorsFromLog, isBot, readContributors } from "../contributors.mjs";

const log = (...rows) => rows.map(([n, e]) => `${n}\t${e}`).join("\n");

describe("contributorsFromLog", () => {
  it("lists each author once, oldest contribution first", () => {
    expect(contributorsFromLog(log(["Jason Sutter", "j@example.com"], ["Kad26", "1+Kad26@users.noreply.github.com"], ["Jason Sutter", "j@example.com"]))).toEqual([
      { name: "Jason Sutter" },
      { name: "Kad26", login: "Kad26" },
    ]);
  });

  it("merges the same person across emails (by name) and the same email across names", () => {
    const list = contributorsFromLog(log(["Jason Sutter", "a@example.com"], ["Jason Sutter", "b@local"], ["J. S.", "a@example.com"], ["jason  sutter", "c@example.com"]));
    expect(list).toEqual([{ name: "Jason Sutter" }]);
  });

  it("merges by GitHub login across a name change, and keeps the login", () => {
    const list = contributorsFromLog(log(["kad", "12+Kad26@users.noreply.github.com"], ["Kad Two", "Kad26@users.noreply.github.com"]));
    expect(list).toEqual([{ name: "kad", login: "Kad26" }]);
  });

  it("skips bots and GitHub itself", () => {
    const list = contributorsFromLog(
      log(
        ["dependabot[bot]", "49699333+dependabot[bot]@users.noreply.github.com"],
        ["github-actions[bot]", "41898282+github-actions[bot]@users.noreply.github.com"],
        ["Dependabot", "support@github.com"],
        ["GitHub", "noreply@github.com"],
        ["renovate-bot", "r@example.com"],
        ["Real Person", "p@example.com"],
      ),
    );
    expect(list).toEqual([{ name: "Real Person" }]);
    expect(isBot("someone", "someone@example.com")).toBe(false);
    expect(isBot("x[bot]", "x@example.com")).toBe(true);
  });

  it("ignores co-author trailers and malformed lines", () => {
    expect(contributorsFromLog("Co-authored-by: Someone Else <s@example.com>\n\n\nno tab here")).toEqual([]);
  });

  it("gives an empty list for no history", () => {
    expect(contributorsFromLog("")).toEqual([]);
    expect(contributorsFromLog(undefined)).toEqual([]);
  });
});

describe("readContributors", () => {
  const dirs = [];
  const tmp = () => {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), "sc-contrib-"));
    dirs.push(d);
    return d;
  };
  afterEach(() => {
    delete process.env.SKETCHCRAFT_NO_UNSHALLOW;
    for (const d of dirs.splice(0)) fs.rmSync(d, { recursive: true, force: true });
  });
  const git = (cwd, ...args) => execFileSync("git", args, { cwd, stdio: "ignore" });
  const commit = (cwd, name, email, file) => {
    fs.writeFileSync(path.join(cwd, file), file);
    git(cwd, "add", file);
    git(cwd, "-c", `user.name=${name}`, "-c", `user.email=${email}`, "-c", "commit.gpgsign=false", "commit", "-q", "-m", file);
  };

  it("is empty outside a git repository, without throwing", () => {
    expect(readContributors(tmp())).toEqual([]);
    expect(readContributors("/definitely/not/a/folder")).toEqual([]);
  });

  it("reads a real history", () => {
    const d = tmp();
    git(d, "init", "-q");
    commit(d, "Ada", "ada@example.com", "a");
    commit(d, "Bot", "1+dependabot[bot]@users.noreply.github.com", "b");
    commit(d, "Grace", "grace@example.com", "c");
    expect(readContributors(d)).toEqual([{ name: "Ada" }, { name: "Grace" }]);
  });

  it("deepens a shallow clone, or gives nothing when it can't", () => {
    const origin = tmp();
    git(origin, "init", "-q");
    commit(origin, "Ada", "ada@example.com", "a");
    commit(origin, "Grace", "grace@example.com", "c");
    const clone = path.join(tmp(), "shallow");
    git(origin, "clone", "-q", "--depth", "1", `file://${origin}`, clone);
    process.env.SKETCHCRAFT_NO_UNSHALLOW = "1";
    expect(readContributors(clone)).toEqual([]); // one commit's author would be a wrong list
    delete process.env.SKETCHCRAFT_NO_UNSHALLOW;
    expect(readContributors(clone)).toEqual([{ name: "Ada" }, { name: "Grace" }]);
  });
});
