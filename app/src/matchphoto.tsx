// Match Photo: drag two red and two green lines along edges in the photo,
// and the yellow dot onto the spot that should be the model's origin. The
// engine works out the camera from where each pair of lines meets.

import { useEffect, useRef, useState } from "react";

type Seg = [number, number, number, number];

export interface PhotoMatch {
  red: [Seg, Seg];
  green: [Seg, Seg];
  origin: [number, number];
  distance: number;
  name: string;
}

const RED = "#e5484d";
const GREEN = "#30a46c";

/** Where two lines meet (null if parallel). */
function meet(a: Seg, b: Seg): [number, number] | null {
  const [px, py, rx, ry] = [a[0], a[1], a[2] - a[0], a[3] - a[1]];
  const [qx, qy, sx, sy] = [b[0], b[1], b[2] - b[0], b[3] - b[1]];
  const den = rx * sy - ry * sx;
  if (Math.abs(den) < 1e-9) return null;
  const t = ((qx - px) * sy - (qy - py) * sx) / den;
  return [px + rx * t, py + ry * t];
}

export function MatchPhoto({ src, width, height, onDone, onCancel, defaultName }: { src: string; width: number; height: number; onDone: (m: PhotoMatch) => void; onCancel: () => void; defaultName: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [name, setName] = useState(defaultName);
  const [distance, setDistance] = useState("15");
  // in photo pixels; the first line of each pair is drawn toward +axis
  const [red, setRed] = useState<[Seg, Seg]>([
    [0.47 * width, 0.8 * height, 0.12 * width, 0.66 * height],
    [0.47 * width, 0.36 * height, 0.12 * width, 0.33 * height],
  ]);
  const [green, setGreen] = useState<[Seg, Seg]>([
    [0.53 * width, 0.8 * height, 0.88 * width, 0.66 * height],
    [0.53 * width, 0.36 * height, 0.88 * width, 0.33 * height],
  ]);
  const [origin, setOrigin] = useState<[number, number]>([0.5 * width, 0.81 * height]);
  const drag = useRef<{ kind: "red" | "green" | "origin"; line: number; end: number } | null>(null);

  useEffect(() => {
    const el = box.current!;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    window.addEventListener("keydown", esc);
    return () => {
      ro.disconnect();
      window.removeEventListener("keydown", esc);
    };
  }, [onCancel]);

  // photo fitted into the box, beside the instructions panel
  const side = 310;
  const k = Math.min((size.w - side) / width, size.h / height) * 0.96;
  const ox = side + (size.w - side - width * k) / 2;
  const oy = (size.h - height * k) / 2;
  const toScreen = (x: number, y: number): [number, number] => [ox + x * k, oy + y * k];
  const toPhoto = (x: number, y: number): [number, number] => [(x - ox) / k, (y - oy) / k];

  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const r = box.current!.getBoundingClientRect();
    const [x, y] = toPhoto(e.clientX - r.left, e.clientY - r.top);
    if (d.kind === "origin") return setOrigin([x, y]);
    const set = d.kind === "red" ? setRed : setGreen;
    set((pair) => {
      const next = pair.map((s) => [...s] as Seg) as [Seg, Seg];
      next[d.line][d.end * 2] = x;
      next[d.line][d.end * 2 + 1] = y;
      return next;
    });
  };

  const pair = (kind: "red" | "green", segs: [Seg, Seg], color: string) => {
    const vp = meet(segs[0], segs[1]);
    return (
      <g key={kind}>
        {segs.map((s, i) => {
          const [ax, ay] = toScreen(s[0], s[1]);
          const [bx, by] = toScreen(s[2], s[3]);
          // dashed guide on toward the vanishing point (or far along the line)
          const far = vp ? toScreen(vp[0], vp[1]) : [bx + (bx - ax) * 20, by + (by - ay) * 20];
          return (
            <g key={i}>
              <line x1={ax} y1={ay} x2={far[0]} y2={far[1]} stroke={color} strokeOpacity={0.5} strokeDasharray="6 5" strokeWidth={1.2} />
              <line x1={ax} y1={ay} x2={bx} y2={by} stroke={color} strokeWidth={2.5} />
              {[0, 1].map((end) => {
                const [cx, cy] = end ? [bx, by] : [ax, ay];
                return (
                  <circle
                    key={end}
                    cx={cx}
                    cy={cy}
                    r={7}
                    fill="#fff"
                    stroke={color}
                    strokeWidth={2.5}
                    style={{ cursor: "grab" }}
                    onPointerDown={(e) => {
                      (e.target as Element).setPointerCapture(e.pointerId);
                      drag.current = { kind, line: i, end };
                    }}
                  />
                );
              })}
            </g>
          );
        })}
      </g>
    );
  };

  const [gx, gy] = toScreen(origin[0], origin[1]);
  return (
    <div className="matchphoto" ref={box}>
      <svg onPointerMove={move} onPointerUp={() => (drag.current = null)}>
        <image href={src} x={ox} y={oy} width={width * k} height={height * k} preserveAspectRatio="none" />
        {pair("red", red, RED)}
        {pair("green", green, GREEN)}
        <circle
          cx={gx}
          cy={gy}
          r={8}
          fill="#f5d90a"
          stroke="#1c1e26"
          strokeWidth={2}
          style={{ cursor: "grab" }}
          onPointerDown={(e) => {
            (e.target as Element).setPointerCapture(e.pointerId);
            drag.current = { kind: "origin", line: 0, end: 0 };
          }}
        />
      </svg>
      <div className="mp-panel">
        <div className="section-title">Match Photo</div>
        <p>
          <span className="mp-key" style={{ background: RED }} />
          Drag both red lines along edges that run one way (say, the top and bottom of a wall), drawn outward from the corner.
        </p>
        <p>
          <span className="mp-key" style={{ background: GREEN }} />
          Green along edges running the other way.
        </p>
        <p>
          <span className="mp-key" style={{ background: "#f5d90a" }} />
          The yellow dot goes where the model's origin should be (a corner on the ground).
        </p>
        <label className="field">
          <span>Scene name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="field">
          <span>Camera to dot (m)</span>
          <input value={distance} onChange={(e) => setDistance(e.target.value)} title="Sets the scale: how far the camera was from the yellow dot" />
        </label>
        <div className="row gap" style={{ marginTop: 10 }}>
          <button className="btn small" onClick={() => onDone({ red, green, origin, distance: parseFloat(distance) || 15, name: name.trim() || defaultName })}>
            Done
          </button>
          <button className="btn small ghost" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
