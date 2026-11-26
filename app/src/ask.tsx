import { useLocale } from "./locale";
// A small in-app text prompt. window.prompt doesn't show in the desktop
// app's web view, so anything that needs a name or a label asks here.

import { useEffect, useRef, useState } from "react";
import { Sheet } from "./sheet";

interface Ask {
  titleId?: string;
  title: string;
  value: string;
  hint?: string;
  /** a yes / no question: no text box, this on the OK button */
  confirm?: string;
  done: (v: string | null) => void;
}

let show: ((a: Ask) => void) | null = null;
// one question at a time: a second one waits its turn (and is answered after the first)
let busy = false;
let current: Ask | null = null;
const waiting: Ask[] = [];
function open(a: Ask) {
  if (!show) return a.done(null);
  if (busy) return void waiting.push(a);
  busy = true;
  const done = a.done;
  const shown: Ask = {
    ...a,
    done: (v) => {
      if (current !== shown) return;
      current = null;
      done(v);
      busy = false;
      const next = waiting.shift();
      if (next) open(next);
    },
  };
  current = shown;
  show(shown);
}

/** Ask for a line of text; null if cancelled. */
export function ask(title: string, value = "", hint?: string, titleId?: string): Promise<string | null> {
  return new Promise((done) => {
    open({ title, value, hint, titleId, done });
  });
}

/** Ask a yes / no question (`ok` labels the yes); true if confirmed. */
export function confirm(title: string, hint: string, ok = "OK"): Promise<boolean> {
  return new Promise((done) => {
    if (show) open({ title, value: "", hint, confirm: ok, done: (v) => done(v != null) });
    else done(false);
  });
}

export function AskHost() {
  const { t } = useLocale();
  const [a, setA] = useState<Ask | null>(null);
  const [text, setText] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    show = (x) => {
      setA(x);
      setText(x.value);
    };
    return () => {
      show = null;
      // the host went away with a question open: answer it (cancelled) so the next one isn't stuck behind it
      current?.done(null);
    };
  }, []);
  useEffect(() => {
    if (a && !a.confirm) setTimeout(() => input.current?.select(), 0);
    if (!a?.confirm) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === "Enter") (e.preventDefault(), setA(null), a.done(""));
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [a]);
  if (!a) return null;
  const finish = (v: string | null) => {
    setA(null);
    a.done(v);
  };
  return (
    <Sheet onClose={() => finish(null)} label={a.titleId ? t(a.titleId) : a.title} className="ask">
      <b>{a.titleId ? t(a.titleId) : a.title}</b>
      {a.hint && <p className="muted small" style={{ whiteSpace: "pre-line" }}>{a.hint}</p>}
      {!a.confirm && <input
        ref={input}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(text);
          if (e.key === "Escape") finish(null);
        }}
      />}
      <div className="row gap">
        <button className="btn small" onClick={() => finish(text)}>
          {a.confirm ?? t("dialog.ok")}
        </button>
        <button className="btn small ghost" onClick={() => finish(null)}>
          {t("dialog.cancel")}
        </button>
      </div>
    </Sheet>
  );
}
