import { describe, expect, it } from "vitest";
import { edgePens, PX_PER_MM } from "../src/viewport/linepens";
import { LINE_STYLES, PEN_WEIGHTS } from "../src/linestyle";

describe("edgePens", () => {
  const seg = (n: number) => [n, 0, 0, n + 1, 0, 0];
  const edges = [...seg(0), ...seg(1), ...seg(2), ...seg(3)];

  it("is null when no edge has a pen of its own", () => {
    expect(edgePens({ edges, edgeStyle: [], edgeStyles: [] })).toBeNull();
    expect(edgePens({ edges })).toBeNull();
  });

  it("splits the segments by pen and keeps the plain ones", () => {
    const r = edgePens({
      edges,
      edgeStyle: [0, 1, 2, 1],
      edgeStyles: [
        { w: 0.7, dash: [], color: [255, 0, 0] },
        { w: 0.13, dash: [3, 1.5], color: [0, 0, 255] },
      ],
    })!;
    expect(r.plain).toEqual(seg(0));
    expect(r.buckets).toHaveLength(2);
    const thick = r.buckets.find((b) => b.color[0] === 255)!;
    expect(thick.pos).toEqual([...seg(1), ...seg(3)]);
    expect(thick.px).toBeCloseTo(0.7 * PX_PER_MM);
    expect(thick.dash).toEqual([]);
    const dashed = r.buckets.find((b) => b.color[2] === 255)!;
    expect(dashed.pos).toEqual(seg(2));
    expect(dashed.dash.length).toBe(2);
    // never thinner than the 1 px default
    expect(dashed.px).toBe(1);
  });

  it("draws a segment with an unknown pen index as plain", () => {
    const r = edgePens({ edges: seg(0), edgeStyle: [7], edgeStyles: [{ w: 0.5, dash: [], color: [0, 0, 0] }] })!;
    expect(r.plain).toEqual(seg(0));
    expect(r.buckets).toEqual([]);
  });
});

describe("pickers", () => {
  it("offer the same pens and styles the engine accepts", () => {
    expect(PEN_WEIGHTS.every((w) => w >= 0.05 && w <= 2)).toBe(true);
    expect(LINE_STYLES.map(([k]) => k)).toEqual(["solid", "dashed", "hidden", "center"]);
  });
});
