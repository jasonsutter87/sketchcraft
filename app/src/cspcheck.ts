// The plugin lockdown (lockdown.ts) removes what a worker could use to reach
// the network or load code, but the window's content security policy is what
// really stops `import()`, `FontFace` and `eval`, which can't be deleted. If a
// webview didn't apply that policy to workers, plugins would have network
// access again. So before the first plugin runs, a probe worker (without the
// lockdown) tries a fetch, an `import()` and an `eval` the policy forbids, and
// each one must be blocked.
//
// "Blocked" is judged by what happens, not only by a violation report: the probes
// are chosen so that each one can only fail if the policy stopped it (a `data:`
// fetch and a `data:` module need no network; `eval("1")` can't fail on its own).
// Some webviews (WebKitGTK, for one) enforce the policy in workers but never fire
// `securitypolicyviolation` there; judging by reports alone turned plugins off on
// them for no reason. A report still counts as blocked where one arrives.
//
// A throw only counts as the policy when it LOOKS like the policy (isPolicyBlock below): a probe that fails
// for another reason (a missing feature, a typo, "not supported in this context") is not proof of anything.

const DATA_MODULE: string = "data:text/javascript,export default 1";

const PROBE = `
const hits = { fetch: false, import: false, eval: false };
let key = "";
self.addEventListener("securitypolicyviolation", (e) => {
  const d = String(e.effectiveDirective || e.violatedDirective || "");
  if (d.startsWith("connect-src") || d.startsWith("script-src")) hits[key] = true;
});
// run f; if it throws, keep what was thrown (the window decides whether that is the policy)
const errors = {};
const probe = async (name, f) => {
  key = name;
  try { await f(); } catch (e) { errors[name] = { name: String(e && e.name), message: String(e && e.message) }; }
  await new Promise((r) => setTimeout(r, 30)); // let a late violation report arrive
};
(async () => {
  await probe("fetch", () => fetch("data:text/plain,csp-probe").then((r) => r.text()));
  await probe("import", () => import("data:text/javascript,export default 1"));
  await probe("eval", () => eval("1"));
  postMessage({ hits, errors });
})();
`;

/**
 * Is this thrown error the policy saying no? `eval` is refused with an EvalError (or a message that names the policy);
 * a blocked `fetch` / `import()` is a TypeError, SecurityError or NetworkError whose message names the policy or says
 * the load failed (a `data:` URL needs no network, so a failed load is the policy). Anything else (ReferenceError,
 * SyntaxError, "not supported", "is not a function") is not proof, so it does not count.
 */
export function isPolicyBlock(
  kind: "fetch" | "import" | "eval",
  err: { name?: string; message?: string } | null | undefined,
  windowEnforces = false,
): boolean {
  if (!err) return false;
  const name = String(err.name ?? "");
  const msg = String(err.message ?? "").toLowerCase();
  if (/not supported|disabled|not a function|is not defined|unexpected token|cannot read/.test(msg)) return false;
  const policy = /content security policy|csp|refused to|violates|unsafe-eval|connect-src|script-src|blocked/.test(msg);
  if (kind === "eval") return name === "EvalError" || (policy && ["EvalError", "TypeError", "SecurityError", "EvalError"].includes(name));
  if (!["TypeError", "SecurityError", "NetworkError"].includes(name)) return false;
  if (policy) return true;
  // The engines' bare "load failed" wording (WebKit, Chrome, Firefox) says nothing about WHY. It counts only for these
  // exact messages, and only when the window itself saw its policy forbid the same kind of load (windowEnforces).
  const bare = /^(failed to fetch|load failed|networkerror when attempting to fetch resource\.?|failed to fetch dynamically imported module.*|importing a module script failed\.?)$/;
  return windowEnforces && bare.test(msg.trim());
}

/** Ask the window (where WebKit does report them) whether its own policy forbids a `data:` fetch and a `data:` import. */
function windowPolicyProbe(): Promise<{ fetch: boolean; import: boolean }> {
  return new Promise((resolve) => {
    const seen = { fetch: false, import: false };
    const on = (e: Event) => {
      const v = e as SecurityPolicyViolationEvent;
      const d = String(v.effectiveDirective || v.violatedDirective || "");
      if (d.startsWith("connect-src")) seen.fetch = true;
      if (d.startsWith("script-src")) seen.import = true;
    };
    try {
      document.addEventListener("securitypolicyviolation", on);
      const quiet = (p: Promise<unknown>) => p.then(() => undefined, () => undefined);
      Promise.all([
        quiet(fetch("data:text/plain,csp-window-probe")),
        quiet(import(/* @vite-ignore */ DATA_MODULE)),
      ]).then(() => setTimeout(() => { document.removeEventListener("securitypolicyviolation", on); resolve(seen); }, 50));
    } catch {
      resolve(seen);
    }
  });
}

let verdict: Promise<{ ok: boolean; why?: string }> | null = null;

/** Does the window's policy hold inside plugin workers? Checked once. */
export function pluginSandboxHolds(): Promise<{ ok: boolean; why?: string }> {
  return (verdict ??= new Promise((resolve) => {
    let url = "";
    try {
      url = URL.createObjectURL(new Blob([PROBE], { type: "text/javascript" }));
      const w = new Worker(url);
      const done = (v: { ok: boolean; why?: string }) => {
        clearTimeout(timer);
        w.terminate();
        URL.revokeObjectURL(url);
        resolve(v);
      };
      const timer = setTimeout(() => done({ ok: false, why: "the plugin safety check didn't finish, so plugins won't run; everything else works as usual" }), 5000);
      w.onerror = () => done({ ok: false, why: "the plugin safety check couldn't run, so plugins won't run; everything else works as usual" });
      w.onmessage = (e) => {
        const d = e.data ?? {};
        const h = d.hits ?? {};
        const errs = d.errors ?? {};
        void windowPolicyProbe().then((win) => {
          const open = (["fetch", "import", "eval"] as const).filter((k) => !h[k] && !isPolicyBlock(k, errs[k], k === "eval" ? false : win[k]));
          done(open.length ? { ok: false, why: `SketchCraft couldn't confirm that plugins are walled off from the network on this system (${open.join(", ")} not blocked), so it won't run them; everything else works as usual` } : { ok: true });
        });
      };
    } catch (e) {
      if (url) URL.revokeObjectURL(url);
      resolve({ ok: false, why: `the safety check couldn't start: ${String(e)}` });
    }
  }));
}

/** Test hook: forget the verdict. */
export function resetPluginSandboxCheck() {
  verdict = null;
}
