// Finds where a tombstone mark goes in Rust: fn bodies (not const fn, not tests, not macro_rules) and, with
// blocks: true, `=>` arms and `else` blocks that have a `{`. A small lexer is enough: it only has to get comments,
// strings, chars and lifetimes right so a brace inside them never counts.

const MODIFIERS = new Set(["pub", "async", "unsafe", "const", "extern", "default"]);

export function lexRust(src) {
  const toks = [];
  const n = src.length;
  let i = 0;
  const isId0 = (c) => /[A-Za-z_]/.test(c) || c > "\x7f";
  const isId = (c) => /[A-Za-z0-9_]/.test(c) || c > "\x7f";
  while (i < n) {
    const c = src[i];
    if (c === " " || c === "\t" || c === "\n" || c === "\r") { i++; continue; }
    if (c === "/" && src[i + 1] === "/") { while (i < n && src[i] !== "\n") i++; continue; }
    if (c === "/" && src[i + 1] === "*") {
      let depth = 1; i += 2;
      while (i < n && depth) {
        if (src[i] === "/" && src[i + 1] === "*") { depth++; i += 2; }
        else if (src[i] === "*" && src[i + 1] === "/") { depth--; i += 2; }
        else i++;
      }
      continue;
    }
    // raw strings r"..", r#".."#, br#".."#
    const raw = /^b?r(#*)"/.exec(src.slice(i, i + 12));
    if (raw) {
      const close = '"' + raw[1];
      const e = src.indexOf(close, i + raw[0].length);
      const end = e < 0 ? n : e + close.length;
      toks.push({ t: "lit", s: "", o: i, e: end }); i = end; continue;
    }
    if (c === '"' || (c === "b" && src[i + 1] === '"')) {
      let j = i + (c === "b" ? 2 : 1);
      while (j < n && src[j] !== '"') j += src[j] === "\\" ? 2 : 1;
      toks.push({ t: "lit", s: "", o: i, e: j + 1 }); i = j + 1; continue;
    }
    if (c === "'" || (c === "b" && src[i + 1] === "'")) {
      const q = c === "b" ? i + 1 : i;
      if (src[q + 1] === "\\") {
        let j = q + 2;
        while (j < n && src[j] !== "'") j++;
        toks.push({ t: "lit", s: "", o: i, e: j + 1 }); i = j + 1; continue;
      }
      const cp = src.codePointAt(q + 1);
      const len = cp > 0xffff ? 2 : 1;
      if (src[q + 1 + len] === "'") { toks.push({ t: "lit", s: "", o: i, e: q + 2 + len }); i = q + 2 + len; continue; }
      // a lifetime or label
      let j = q + 1;
      while (j < n && isId(src[j])) j++;
      i = j; continue;
    }
    if (isId0(c)) {
      let j = i + 1;
      while (j < n && isId(src[j])) j++;
      toks.push({ t: "id", s: src.slice(i, j), o: i, e: j }); i = j; continue;
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      while (j < n && /[0-9A-Za-z_.]/.test(src[j]) && !(src[j] === "." && !/[0-9]/.test(src[j + 1] ?? ""))) j++;
      toks.push({ t: "lit", s: "", o: i, e: j }); i = j; continue;
    }
    toks.push({ t: "p", s: c, o: i, e: i + 1 }); i++;
  }
  return toks;
}

export function findRust(src, { blocks = false } = {}) {
  const toks = lexRust(src);
  const n = toks.length;
  // brace matching
  const match = new Map();
  const st = [];
  for (let i = 0; i < n; i++) {
    if (toks[i].t !== "p") continue;
    if (toks[i].s === "{") st.push(i);
    else if (toks[i].s === "}" && st.length) match.set(st.pop(), i);
  }
  const isP = (i, s) => i >= 0 && i < n && toks[i].t === "p" && toks[i].s === s;
  const isId = (i, s) => i >= 0 && i < n && toks[i].t === "id" && (s === undefined || toks[i].s === s);

  /** Walk back over modifiers and attributes before an item keyword: its first token and its attributes' text. */
  function walkBack(i) {
    let k = i - 1, hasConst = false;
    const attrs = [];
    for (;;) {
      if (k >= 0 && toks[k].t === "id" && MODIFIERS.has(toks[k].s)) { if (toks[k].s === "const") hasConst = true; k--; }
      else if (k >= 0 && toks[k].t === "lit") k--; // extern "C"
      else if (isP(k, ")")) {
        let d = 0, j = k;
        for (; j >= 0; j--) { if (isP(j, ")")) d++; else if (isP(j, "(") && --d === 0) break; }
        if (isId(j - 1, "pub")) k = j - 1; else break;
      } else if (isP(k, "]")) {
        let d = 0, j = k;
        for (; j >= 0; j--) { if (isP(j, "]")) d++; else if (isP(j, "[") && --d === 0) break; }
        if (isP(j - 1, "#")) {
          attrs.push(toks.slice(j + 1, k).map((t) => t.s).join(" "));
          k = j - 2;
        } else break;
      } else break;
    }
    return { first: k + 1, attrs, hasConst };
  }
  const isTestAttr = (a) => a === "test" || /^cfg \( test \)$/.test(a) || /^tokio :: test/.test(a);

  const out = [];
  const stack = []; // { name, kind, skip }
  const pendingAt = new Map(); // index of `{` -> entry
  const scopeNames = () => stack.map((e) => e.name).filter(Boolean);
  const skipping = () => stack.some((e) => e.skip);

  for (let i = 0; i < n; i++) {
    const tk = toks[i];
    if (tk.t === "id" && tk.s === "macro_rules" && isP(i + 1, "!")) {
      let j = i + 2;
      while (j < n && !isP(j, "{") && !isP(j, "(") && !isP(j, "[")) j++;
      // skip the whole definition (its body is a template, not code)
      if (isP(j, "{")) { i = match.get(j) ?? j; continue; }
      continue;
    }
    if (tk.t === "id" && (tk.s === "impl" || tk.s === "trait" || tk.s === "mod") && !(isP(i - 1, ">") && isP(i - 2, "-"))) {
      let d = 0, k = i + 1;
      for (; k < n; k++) {
        if (isP(k, "(") || isP(k, "[")) d++; else if (isP(k, ")") || isP(k, "]")) d--;
        else if (d === 0 && (isP(k, "{") || isP(k, ";"))) break;
      }
      if (isP(k, "{")) {
        const { attrs } = walkBack(i);
        const name = tk.s === "mod" ? toks[i + 1].s : src.slice(toks[i + (tk.s !== "mod" ? 1 : 0)].o, toks[k].o).replace(/\s+/g, " ").replace(/<[^<>]*>/g, "").trim().slice(0, 60);
        pendingAt.set(k, { name, kind: tk.s, skip: attrs.some(isTestAttr) });
      }
      continue;
    }
    if (tk.t === "id" && tk.s === "fn" && toks[i + 1]?.t === "id") {
      const name = toks[i + 1].s;
      let d = 0, k = i + 2;
      for (; k < n; k++) {
        if (isP(k, "(") || isP(k, "[")) d++; else if (isP(k, ")") || isP(k, "]")) d--;
        else if (d === 0 && (isP(k, "{") || isP(k, ";"))) break;
      }
      const { first, attrs, hasConst } = walkBack(i);
      const skip = hasConst || attrs.some(isTestAttr);
      if (isP(k, "{")) {
        const symbol = [...scopeNames(), name].join("::");
        if (!skip && !skipping()) {
          const close = match.get(k) ?? k;
          out.push({ bodyOpen: toks[k].o, start: toks[i].o, end: toks[close].o, symbol, kind: "fn", span: [toks[first].o, toks[close].e] });
        }
        pendingAt.set(k, { name, kind: "fn", skip });
        i = k - 1; // the signature has no more items; the `{` is handled below
      }
      continue;
    }
    if (isP(i, "{")) {
      const entry = pendingAt.get(i) ?? { name: null, kind: "block", skip: false };
      const inFn = stack.some((e) => e.kind === "fn");
      if (blocks && inFn && !skipping() && !entry.skip && entry.kind === "block") {
        const scope = scopeNames().join("::");
        if (isP(i - 1, ">") && isP(i - 2, "=") && toks[i - 2].e === toks[i - 1].o) {
          out.push({ bodyOpen: toks[i].o, start: toks[i].o, end: toks[match.get(i) ?? i].o, symbol: `${scope}#arm`, kind: "arm" });
        } else if (isId(i - 1, "else")) {
          out.push({ bodyOpen: toks[i].o, start: toks[i - 1].o, end: toks[match.get(i) ?? i].o, symbol: `${scope}#else`, kind: "else" });
        }
      }
      stack.push({ ...entry, skip: entry.skip });
    } else if (isP(i, "}")) {
      stack.pop();
    }
  }
  out.sort((a, b) => a.bodyOpen - b.bodyOpen);
  return out;
}
