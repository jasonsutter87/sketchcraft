import { describe, expect, it } from "vitest";
import english from "../src/locale-en";
import italian from "../src/locale-it";
import { buildMenuActions } from "../src/menuactions";
import { after, apply, dragDelta, fromTriangles, hitPlane, homography, IDENT, invert, mapPins, opDelta, pictureMatrix, pinsFor, planeOf, toFace, toWorld, type Frame, type V2 } from "../src/positiontexture";

const close = (a: V2, b: V2, eps = 1e-6) => {
  expect(a[0]).toBeCloseTo(b[0], 5);
  expect(a[1]).toBeCloseTo(b[1], 5);
  void eps;
};

// a 4 x 2 m floor face (z = 0, facing up); a tile of 2 x 1 m laid from the origin
const floor: Frame = {
  tile: [2, 1],
  normal: [0, 0, 1],
  points: [[[0, 0, 0], [4, 0, 0], [4, 2, 0], [0, 2, 0]]],
  picture: [[[0, 0], [4, 0], [4, 2], [0, 2]]],
};

describe("the face's plane and the pins", () => {
  it("fits picture = L(face) from the corners, and the pins sit on the tile under the face's middle", () => {
    const pl = planeOf(floor)!;
    expect(pl).toBeTruthy();
    const pins = pinsFor(pl, floor)!;
    // the middle (2, 1) is on the tile whose corner is at picture (2, 1): u 1..2, v 1..2 of the (2 x 1) tile grid
    const w = (s: V2) => toWorld(pl, s);
    for (const [pin, want] of [[pins.r, [2, 1]], [pins.g, [4, 1]], [pins.b, [2, 2]], [pins.y, [4, 2]]] as [V2, V2][]) {
      const p = w(pin);
      close([p[0], p[1]], want);
    }
  });

  it("is null for a face on a line", () => {
    expect(planeOf({ ...floor, points: [[[0, 0, 0], [1, 0, 0], [2, 0, 0]]], picture: [[[0, 0], [1, 0], [2, 0]]] })).toBeNull();
  });

  it("round-trips a point through the plane, and a ray hits it", () => {
    const pl = planeOf(floor)!;
    close(toFace(pl, toWorld(pl, [1.25, -0.5])) as V2, [1.25, -0.5]);
    const s = hitPlane(pl, [1, 1, 5], [0, 0, -1])!;
    const w = toWorld(pl, s);
    close([w[0], w[1]], [1, 1]);
    expect(hitPlane(pl, [1, 1, 5], [0, 0, 1])).toBeNull();
    expect(hitPlane(pl, [1, 1, 5], [1, 0, 0])).toBeNull();
  });
});

describe("what each pin does", () => {
  const pl = planeOf(floor)!;
  const pins = pinsFor(pl, floor)!;
  const delta = (kind: Parameters<typeof dragDelta>[0], by: V2) => dragDelta(kind, pins, pins.r, [pins.r[0] + by[0], pins.r[1] + by[1]]);

  it("red and the picture itself move everything by the drag", () => {
    for (const k of ["red", "move"] as const) {
      const m = mapPins(delta(k, [0.5, -0.25]), pins);
      close(m.g, [pins.g[0] + 0.5, pins.g[1] - 0.25]);
      close(m.y, [pins.y[0] + 0.5, pins.y[1] - 0.25]);
    }
  });

  it("green turns and scales about red so the green pin follows the pointer", () => {
    const d = dragDelta("green", pins, pins.g, [pins.g[0], pins.g[1] + 2]);
    const m = mapPins(d, pins);
    close(m.r, pins.r);
    close(m.g, [pins.g[0], pins.g[1] + 2]);
    // the picture stayed square (a similarity): the blue pin turned with it
    const a = [m.g[0] - m.r[0], m.g[1] - m.r[1]];
    const b = [m.b[0] - m.r[0], m.b[1] - m.r[1]];
    expect(a[0] * b[0] + a[1] * b[1]).toBeCloseTo((pins.g[0] - pins.r[0]) * (pins.b[0] - pins.r[0]) + (pins.g[1] - pins.r[1]) * (pins.b[1] - pins.r[1]), 5);
  });

  it("blue follows the pointer and keeps green where it was", () => {
    const m = mapPins(dragDelta("blue", pins, pins.b, [pins.b[0] + 0.7, pins.b[1] + 0.3]), pins);
    close(m.b, [pins.b[0] + 0.7, pins.b[1] + 0.3]);
    close(m.g, pins.g);
    close(m.r, pins.r);
  });

  it("yellow follows the pointer and keeps the other diagonal", () => {
    const m = mapPins(dragDelta("yellow", pins, pins.y, [pins.y[0] + 0.4, pins.y[1] - 0.2]), pins);
    close(m.y, [pins.y[0] + 0.4, pins.y[1] - 0.2]);
    close(m.r, pins.r);
    // b - g is unchanged
    close([m.b[0] - m.g[0], m.b[1] - m.g[1]], [pins.b[0] - pins.g[0], pins.b[1] - pins.g[1]]);
  });

  it("a drag that would flatten the picture does nothing", () => {
    const a = [pins.g[0] - pins.r[0], pins.g[1] - pins.r[1]];
    expect(dragDelta("green", pins, pins.g, [pins.g[0] - a[0], pins.g[1] - a[1]])).toEqual(IDENT);
    expect(dragDelta("blue", pins, pins.b, [pins.b[0] + a[0] / 2 - (pins.b[0] - pins.r[0]), pins.b[1] + a[1] / 2 - (pins.b[1] - pins.r[1])])).toEqual(IDENT);
  });

  it("drags compose: the second starts from where the first left the pins", () => {
    const d1 = delta("red", [1, 0]);
    const p1 = mapPins(d1, pins);
    const d2 = dragDelta("green", p1, p1.g, [p1.g[0] + 1, p1.g[1]]);
    const m = mapPins(after(d2, d1), pins);
    close(m.r, [pins.r[0] + 1, pins.r[1]]);
    close(m.g, [pins.g[0] + 2, pins.g[1]]);
  });
});

describe("the change the engine is told", () => {
  const pl = planeOf(floor)!;
  const pins = pinsFor(pl, floor)!;

  it("a move of the picture by 0.5 m along the face is a shift of the coordinates by -0.5 m (the picture moves, the corners read it back)", () => {
    // the picture's content moves by D in face space; a corner then reads the value the content had where it came from: L(D^-1 s)
    const o = toFace(pl, [0, 0, 0]);
    const to = toFace(pl, [0.5, 0, 0]);
    const a = pictureMatrix(pl, [1, 0, 0, 1, to[0] - o[0], to[1] - o[1]])!;
    // here the picture is laid 1:1 on the floor, so the corner at world (1, 1) now reads what was at (0.5, 1)
    close(apply(a, [1, 1]), [0.5, 1]);
  });

  it("is the identity for no change, and exact for a turn about a pin", () => {
    expect(pictureMatrix(pl, IDENT)!.map((v) => Math.round(v * 1e9) / 1e9)).toEqual([1, 0, 0, 1, 0, 0]);
    const turn = dragDelta("green", pins, pins.g, [pins.r[0], pins.r[1] + 2]);
    const a = pictureMatrix(pl, turn)!;
    // the face corner under the red pin keeps its coordinates
    const redPic = apply(pl.L, pins.r);
    close(apply(a, redPic), redPic);
  });

  it("flips and turns about the middle of the picture", () => {
    const mid: V2 = [(pins.r[0] + pins.y[0]) / 2, (pins.r[1] + pins.y[1]) / 2];
    const fh = mapPins(opDelta("flipH", pins)!, pins);
    close(fh.r, pins.g);
    close(fh.g, pins.r);
    close(fh.b, pins.y);
    const fv = mapPins(opDelta("flipV", pins)!, pins);
    close(fv.r, pins.b);
    close(fv.y, pins.g);
    const r180 = mapPins(opDelta("rot180", pins)!, pins);
    close(r180.r, pins.y);
    close(r180.b, pins.g);
    const r90 = mapPins(opDelta("rot90", pins)!, pins);
    close([(r90.r[0] + r90.y[0]) / 2, (r90.r[1] + r90.y[1]) / 2], mid);
    // four quarter turns are no turn
    let d = IDENT;
    for (let i = 0; i < 4; i++) d = after(opDelta("rot90", mapPins(d, pins))!, d);
    close(apply(d, pins.g), pins.g);
    // and 90 then 270 are the same as 180 pins-wise
    const t = after(opDelta("rot270", mapPins(opDelta("rot90", pins)!, pins))!, opDelta("rot90", pins)!);
    close(apply(t, pins.r), apply(IDENT, pins.r));
  });

  it("inverts and builds triangle maps", () => {
    const m = fromTriangles([[0, 0], [1, 0], [0, 1]], [[2, 3], [4, 3], [2, 7]])!;
    close(apply(m, [1, 1]), [4, 7]);
    close(apply(invert(m)!, [4, 7]), [1, 1]);
    expect(fromTriangles([[0, 0], [1, 1], [2, 2]], [[0, 0], [1, 0], [0, 1]])).toBeNull();
  });
});

describe("the picture drawn on screen", () => {
  it("maps the box corners to the four screen points", () => {
    const dst: [V2, V2, V2, V2] = [[10, 20], [210, 30], [190, 140], [0, 120]];
    const h = homography(100, 100, dst)!;
    const at = (x: number, y: number): V2 => {
      const w = h[3] * x + h[7] * y + h[15];
      return [(h[0] * x + h[4] * y + h[12]) / w, (h[1] * x + h[5] * y + h[13]) / w];
    };
    close(at(0, 0), dst[0]);
    close(at(100, 0), dst[1]);
    close(at(100, 100), dst[2]);
    close(at(0, 100), dst[3]);
    expect(homography(100, 100, [[0, 0], [1, 1], [2, 2], [3, 3]])).toBeNull();
  });
});

describe("where it is offered, and what it says", () => {
  it("has an English and an Italian string for everything it shows", () => {
    const ids = ["menu.positionTexture", "context.positionTexture", "pos.select", "pos.nopicture", "pos.title", "pos.hint", "pos.done", "pos.cancel", "pos.reset", "pos.flipH", "pos.flipV", "pos.rot90", "pos.rot180", "pos.rot270", "pos.cantpin"];
    for (const k of ["red", "green", "blue", "yellow"]) ids.push(`pos.pin.${k}`, `pos.label.${k}`);
    for (const loc of [english, italian] as Record<string, string>[]) for (const id of ids) expect(loc[id], id).toBeTruthy();
    expect(italian["pos.title"]).not.toBe(english["pos.title"]);
  });

  it("is a Tools menu action that needs a selection", () => {
    const calls: string[] = [];
    const ctx = (total: number) =>
      ({
        selection: { total, faces: total, groups: 0, instances: 0 },
        onPaper: false,
        styles: {},
        leftTabs: [],
        file: {},
        edit: {},
        open: new Proxy({}, { get: (_t, k) => () => calls.push(String(k)) }),
      }) as never;
    expect(buildMenuActions(ctx(0))["tools.positiontexture"].enabled).toBe(false);
    const on = buildMenuActions(ctx(1));
    expect(on["tools.positiontexture"].enabled).toBe(true);
    on["tools.positiontexture"].run();
    expect(calls).toEqual(["positionTexture"]);
  });
});
