import { describe, expect, it } from "vitest";
import { FACE_OFFSET, FACE_OFFSET_UNDER_PICTURES, PICTURE_OFFSET, pictureLead } from "../src/viewport/depth";

describe("depth offsets of textured faces (#222)", () => {
  it("the base under a picture is pushed well behind it, so perspective depth steps can't let it show through", () => {
    // one unit (the plain-face offset against the picture's) flickered in perspective; the picture must lead by many steps
    expect(pictureLead(FACE_OFFSET, PICTURE_OFFSET)).toBeLessThan(4);
    expect(pictureLead()).toBeGreaterThanOrEqual(8);
    expect(FACE_OFFSET_UNDER_PICTURES.factor).toBeGreaterThan(PICTURE_OFFSET.factor);
  });
  it("the picture stays at the depth of edges and lines (it never hides them)", () => {
    expect(PICTURE_OFFSET).toEqual({ factor: 0, units: 0 });
  });
});
