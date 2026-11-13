// A small in-app text prompt. window.prompt doesn't show in the desktop
// app's web view, so anything that needs a name or a label asks here.

import { useEffect, useRef, useState } from "react";

interface Ask {
  title: string;
  value: string;
  hint?: string;
  done: (v: string | null) => void;
}

let show: ((a: Ask) => void) | null = null;

/** Ask for a line of text; null if cancelled. */
export function ask(title: string, value = "", hint?: string): Promise<string | null> {
  return new Promise((done) => {
    if (show) show({ title, value, hint, done });
    else done(null);
  });
}

export function AskHost() {
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
    };
  }, []);
  useEffect(() => {
    if (a) setTimeout(() => input.current?.select(), 0);
  }, [a]);
  if (!a) return null;
  const finish = (v: string | null) => {
    a.done(v);
    setA(null);
  };
  return (
    <div className="sheet-backdrop" onClick={() => finish(null)}>
      <div className="sheet ask" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={a.title}>
        <b>{a.title}</b>
        {a.hint && <p className="muted small">{a.hint}</p>}
        <input
          ref={input}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") finish(text);
            if (e.key === "Escape") finish(null);
          }}
        />
        <div className="row gap">
          <button className="btn small" onClick={() => finish(text)}>
            OK
          </button>
          <button className="btn small ghost" onClick={() => finish(null)}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
