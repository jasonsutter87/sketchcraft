// The Library panel: assets, the model's components and saved parts, with thumbnails.
import { useEffect, useState } from "react";
import { ask } from "./ask";
import { ASSETS, exec, type Scene } from "./engine";
import { FIXTURE_GROUPS, FIXTURES, sizeText } from "./fixtures";
import { TEXTURING_SET, setSizeText } from "./texturing-set";
import { useLocale } from "./locale";
import { pickOpen } from "./files";
import { Icons } from "./icons";
import { type Run } from "./panelkit";

/** Thumbnails, fetched one at a time and kept for the session. */
const thumbs = new Map<string, string>();
const waiting = new Map<string, ((d: string) => void)[]>();
const queue: { key: string; req: Record<string, unknown> }[] = [];
let busy = false;
async function pump() {
  if (busy) return;
  busy = true;
  while (queue.length) {
    const { key, req } = queue.shift()!;
    const r = await exec({ cmd: "thumbnail", size: 160, ...req }).catch(() => null);
    const data = r?.ok ? (r.data as string) : "";
    thumbs.set(key, data);
    for (const f of waiting.get(key) ?? []) f(data);
    waiting.delete(key);
  }
  busy = false;
}

function Thumb({ k, req }: { k: string; req: Record<string, unknown> }) {
  const [src, setSrc] = useState(() => thumbs.get(k) ?? null);
  useEffect(() => {
    if (thumbs.has(k)) return setSrc(thumbs.get(k)!);
    let live = true;
    waiting.set(k, [...(waiting.get(k) ?? []), (d) => live && setSrc(d)]);
    if (!queue.some((q) => q.key === k)) queue.push({ key: k, req });
    void pump();
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [k]);
  return src ? <img className="thumb" src={src} alt="" draggable={false} /> : <span className="thumb loading" />;
}

interface LibEntry {
  path: string;
  name: string;
  collection: string;
}

export function Library({ scene, place, placing, run, onToast }: { scene: Scene; place: (k: string | null) => void; placing: string | null; run: Run; onToast: (t: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const [q, setQ] = useState("");
  const [lib, setLib] = useState<{ dir: string; entries: LibEntry[] } | null>(null);
  const reload = async () => {
    const r = await exec({ cmd: "library" });
    if (r.ok) setLib({ dir: r.dir as string, entries: r.entries as LibEntry[] });
  };
  useEffect(() => {
    void reload();
  }, []);
  const match = (name: string) => !q.trim() || name.toLowerCase().includes(q.trim().toLowerCase());
  // in the model: placed things first; parts of other components only when searching
  const used = scene.components.filter((c) => match(c.name) && (c.placed > 0 || (q.trim() !== "" && (c.inside ?? 0) > 0)));
  const unused = scene.components.filter((c) => c.placed === 0 && (c.inside ?? 0) === 0);
  const parts = scene.components.filter((c) => c.placed === 0 && (c.inside ?? 0) > 0).length;
  const collections = new Map<string, LibEntry[]>();
  for (const e of lib?.entries ?? []) if (match(e.name)) collections.set(e.collection, [...(collections.get(e.collection) ?? []), e]);
  // "people", "scale figure" and the like find every figure
  const figureWords = /^(people|person|scale|figure|scale figures?|figures)$/;
  const starter = ASSETS.filter((a) => !a.figure && match(a.name));
  const figures = ASSETS.filter((a) => a.figure && (match(a.name) || figureWords.test(q.trim().toLowerCase())));

  // fixtures & furniture: found by name or by room ("bath", "kitchen"...), in the current language
  const fixtureGroups = FIXTURE_GROUPS.map((g) => ({
    id: g,
    label: t(`fixtures.${g}`),
    items: FIXTURES.filter((f) => f.group === g && (match(t(`fixture.${f.key}`)) || match(t(`fixtures.${g}`)))),
  })).filter((g) => g.items.length > 0);

  // the texturing test set: found by name or by the collection name
  const setItems = TEXTURING_SET.filter((m) => match(t(`texset.${m.key}`)) || match(t("texset.title")));

  const tile = (key: string, name: string, thumbKey: string, req: Record<string, unknown>, title: string, extra?: React.ReactNode) => (
    <div key={key} className={`tile comp ${placing === key ? "on" : ""}`} title={title}>
      <button className="tile-hit" onClick={() => place(placing === key ? null : key)}>
        <span className="tile-art thumb-art">
          <Thumb k={thumbKey} req={req} />
        </span>
        <span className="tile-name">{name}</span>
      </button>
      {extra}
    </div>
  );

  const saveToLibrary = async (id: number, name: string) => {
    const collection = await ask(`Save "${name}" to my library`, "", "Collection (a folder in your library), or leave blank.");
    if (collection === null) return;
    const r = await run({ cmd: "save_to_library", component: id, collection });
    if (r.ok) {
      onToast(`Saved to your library: ${String(r.path).split("/").slice(-2).join("/")}`);
      void reload();
    }
  };
  const addFiles = async () => {
    const path = await pickOpen();
    if (!path) return;
    const r = await run({ cmd: "library_add", paths: [path] });
    if (r.ok) {
      onToast("Added to your library");
      void reload();
    }
  };

  return (
    <>
      <input className="lib-search" placeholder="Search components" value={q} onChange={(e) => setQ(e.target.value)} />

      <div className="section-title sub row-between">
        <span>In this model</span>
        {unused.length > 0 && (
          <button className="link" onClick={() => run({ cmd: "purge" }).then((r) => r.ok && onToast(`Removed ${r.removed} unused component${r.removed === 1 ? "" : "s"}`))} title="Remove components nothing places">
            Purge {unused.length} unused
          </button>
        )}
      </div>
      {used.length ? (
        <div className="tiles">
          {used.map((c) =>
            tile(
              `component:${c.id}`,
              c.name,
              `c${c.id}:${c.placed}:${scene.faces.length}`,
              { component: c.id },
              `${c.name}: ${c.placed} placed${c.inside ? `, ${c.inside} inside other components` : ""}. Click, then click in the model to place another.`,
              <>
                <span className="tile-count">{c.placed}×</span>
                <button className="tile-act" title="Save to my library" onClick={() => saveToLibrary(c.id, c.name)}>
                  <Icons.plus />
                </button>
              </>
            )
          )}
        </div>
      ) : (
        <p className="hint-text">{q ? "Nothing here matches." : "Components you make (G) or place show up here."}</p>
      )}
      {parts > 0 && !q && <p className="hint-text">{parts} more are parts inside these. Search to find them.</p>}

      <div className="section-title sub row-between">
        <span>My library</span>
        <span className="row gap-s">
          <button className="link" onClick={addFiles} title="Copy a .skp, .fbx, .obj or .sketchcraft file into your library">
            Add file…
          </button>
          <button className="link" onClick={() => run({ cmd: "reveal", path: lib?.dir })} title={lib?.dir}>
            Open folder
          </button>
          <button className="link" onClick={reload} title="Look for new files">
            ↻
          </button>
        </span>
      </div>
      {collections.size === 0 && (
        <p className="hint-text">
          {q ? "Nothing in your library matches." : "Your own components: save one from the model (+ on its tile), or drop .skp, .fbx, .obj or .sketchcraft files into the library folder. Subfolders become collections."}
        </p>
      )}
      {[...collections.entries()].map(([col, items]) => (
        <details key={col || "_top"} className="collection" open>
          <summary>{col || "Unsorted"}</summary>
          <div className="tiles">{items.map((e) => tile(`file:${e.path}`, e.name, `f:${e.path}`, { path: e.path }, `${e.name}. Click, then click in the model to place it.`))}</div>
        </details>
      ))}

      {starter.length > 0 && (
        <details className="collection" open={!lib?.entries.length}>
          <summary>Starter</summary>
          <div className="tiles">{starter.map((a) => tile(a.key, a.name, `a:${a.key}`, { asset: a.key }, `Place ${a.name}: click in the model`))}</div>
        </details>
      )}
      {fixtureGroups.length > 0 && (
        <details className="collection fixtures" open={q.trim() !== ""}>
          <summary title={t("fixtures.tip")}>{t("fixtures.title")}</summary>
          {fixtureGroups.map((g) => (
            <details key={g.id} className="collection" open data-group={g.id}>
              <summary>{g.label}</summary>
              <div className="tiles">
                {g.items.map((f) =>
                  tile(f.key, t(`fixture.${f.key}`), `a:${f.key}`, { asset: f.key }, `${t(`fixture.${f.key}`)}, ${sizeText(f)}. ${t("fixtures.place")}`)
                )}
              </div>
            </details>
          ))}
        </details>
      )}
      {setItems.length > 0 && (
        <details className="collection texset" open={q.trim() !== ""}>
          <summary title={t("texset.tip")}>{t("texset.title")}</summary>
          <div className="tiles">
            {setItems.map((m) =>
              tile(m.key, t(`texset.${m.key}`), `a:${m.key}`, { asset: m.key }, `${t(`texset.${m.key}`)}, ${setSizeText(m)}: ${t(`texset.use.${m.key}`)}. ${t("texset.place")}`)
            )}
          </div>
        </details>
      )}
      {figures.length > 0 && (
        <details className="collection" open={!lib?.entries.length}>
          <summary title="People, cyclists and flat cars for a sense of size along paths and streets">Scale figures</summary>
          <div className="tiles">{figures.map((a) => tile(a.key, a.name, `a:${a.key}`, { asset: a.key }, `Place ${a.name}: click in the model${a.key.endsWith("_flat") ? ". Flat, for elevations: rotate it to line up with the road" : ". It always turns to face you"}`))}</div>
        </details>
      )}
      <p className="hint-text">Components share their geometry: edit one (double-click it) and every copy changes.</p>
    </>
  );
}
