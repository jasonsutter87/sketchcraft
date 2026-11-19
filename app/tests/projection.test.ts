import { describe, expect, it } from "vitest";
import english from "../src/locale-en";
import italian from "../src/locale-it";
import type { MaterialInfo } from "../src/engine";
import { buildMenuActions } from "../src/menuactions";
import { clampScale, defaultFit, fitLabelId, initialState, modeHintId, parseNumber, pickMaterial, PROJ_MODES, projectCommand, targetIds, viewDirection, withMode, wrapDegrees } from "../src/projection";
import { EMPTY } from "../src/selection";

const mat = (name: string, textured: boolean): MaterialInfo => ({ name, color: [1, 1, 1], texture: textured ? { data: "data:image/png;base64,AA==", width: 1, height: 1, project: null } : null });

describe("the Project texture panel's state", () => {
  it("starts planar from the camera and the others upright", () => {
    expect(initialState("planar", null).axis).toBe("view");
    for (const m of ["box", "cylinder", "sphere"] as const) expect(initialState(m, null).axis).toBe("z");
  });

  it("fits once around for cylinder and sphere, and tiles for planar and box, unless told", () => {
    expect(PROJ_MODES.filter(defaultFit)).toEqual(["cylinder", "sphere"]);
    expect(fitLabelId("sphere")).toBe("proj.fit.around");
    expect(fitLabelId("box")).toBe("proj.fit.selection");
  });

  it("keeps size and turn when the mode changes, but resets the fit and fixes an axis that no longer exists", () => {
    const s = { ...initialState("planar", "Label"), scale: 2, rotation: 30, fit: true };
    const c = withMode(s, "cylinder");
    expect(c).toMatchObject({ mode: "cylinder", scale: 2, rotation: 30, material: "Label", fit: null, axis: "z" });
    expect(withMode({ ...c, axis: "x" }, "box").axis).toBe("x");
    expect(withMode(c, "planar").axis).toBe("view");
  });

  it("limits the size and wraps the turn", () => {
    expect(clampScale(0)).toBe(0.05);
    expect(clampScale(1000)).toBe(50);
    expect(clampScale(NaN)).toBe(1);
    expect(wrapDegrees(190)).toBe(-170);
    expect(wrapDegrees(-190)).toBe(170);
    expect(wrapDegrees(360)).toBe(0);
    expect(wrapDegrees(180)).toBe(180);
    expect(wrapDegrees(Infinity)).toBe(0);
  });

  it("projects onto faces, groups and placed components together", () => {
    expect(targetIds({ ...EMPTY, faces: [1, 2], groups: [3], instances: [4] })).toEqual([1, 2, 3, 4]);
    expect(targetIds({ ...EMPTY, edges: [9] })).toEqual([]);
  });

  it("starts on the picture the selection already shows, else the first picture, else none", () => {
    const ms = [mat("Plain", false), mat("Brick", true), mat("Wood", true)];
    expect(pickMaterial(ms, ["Plain", "Wood"], null)).toBe("Wood");
    expect(pickMaterial(ms, [null], null)).toBe("Brick");
    expect(pickMaterial(ms, [null], "Wood")).toBe("Wood");
    expect(pickMaterial(ms, [], "Plain")).toBe("Brick");
    expect(pickMaterial([mat("Plain", false)], [], null)).toBeNull();
  });

  it("reads typed numbers, with a comma too", () => {
    expect(parseNumber("1.5")).toBe(1.5);
    expect(parseNumber(" 2,5 ")).toBe(2.5);
    expect(parseNumber("")).toBeNull();
    expect(parseNumber("abc")).toBeNull();
  });
});

describe("the engine command it makes", () => {
  const cam = { eye: [0, -10, 0], target: [0, 0, 0] };

  it("is nothing without a selection", () => {
    expect(projectCommand(initialState("box", "L"), [], cam)).toBeNull();
  });

  it("names the faces, the mode, the picture, size and turn", () => {
    const c = projectCommand({ ...initialState("cylinder", "Label"), scale: 1.5, rotation: 270 }, [7, 8], cam)!;
    expect(c).toEqual({ cmd: "texture_project", faces: [7, 8], mode: "cylinder", material: "Label", scale: 1.5, rotation: -90, axis: [0, 0, 1] });
  });

  it("sends the camera's direction for a planar projection from this view", () => {
    const c = projectCommand(initialState("planar", "L"), [1], cam)!;
    expect(c.axis).toEqual([0, 1, 0]);
    // without a camera the engine's own default (the biggest face) is left to decide
    expect("axis" in projectCommand(initialState("planar", "L"), [1])!).toBe(false);
    expect(viewDirection({ eye: [1, 1, 1], target: [1, 1, 1] })).toBeNull();
  });

  it("sends the fit and the center only when they were chosen", () => {
    const plain = projectCommand(initialState("box", "L"), [1], cam)!;
    expect("fit" in plain).toBe(false);
    expect("origin" in plain).toBe(false);
    const c = projectCommand({ ...initialState("box", "L"), fit: false, origin: [1, 2, 3] }, [1], cam)!;
    expect(c).toMatchObject({ fit: false, origin: [1, 2, 3] });
  });

  it("projects along the chosen planar direction, and stands the others on the chosen axis", () => {
    expect(projectCommand({ ...initialState("planar", "L"), axis: "z" }, [1], cam)!.axis).toEqual([0, 0, -1]);
    expect(projectCommand({ ...initialState("planar", "L"), axis: "y" }, [1], cam)!.axis).toEqual([0, 1, 0]);
    expect(projectCommand({ ...initialState("sphere", "L"), axis: "x" }, [1], cam)!.axis).toEqual([1, 0, 0]);
  });

  it("clamps what was typed", () => {
    const c = projectCommand({ ...initialState("box", "L"), scale: 0 }, [1], cam)!;
    expect(c.scale).toBe(0.05);
  });
});

describe("where it is offered, and what it says", () => {
  it("has an English and an Italian string for everything the panel shows", () => {
    const ids = ["menu.projectTexture", "context.projectTexture", "proj.title", "proj.count", "proj.select", "proj.nopicture", "proj.mode", "proj.material", "proj.none", "proj.scale", "proj.rotation", "proj.more", "proj.axis", "proj.origin", "proj.origin.hint", "proj.apply", "proj.cancel", "proj.clear", "proj.clear.tip", "proj.done", "proj.cleared", "proj.fit.around", "proj.fit.selection"];
    for (const m of PROJ_MODES) ids.push(`proj.mode.${m}`, modeHintId(m));
    for (const a of ["z", "x", "y"]) ids.push(`proj.axis.${a}`);
    for (const a of ["view", "z", "y", "x"]) ids.push(`proj.axis.planar.${a}`);
    for (const loc of [english, italian] as Record<string, string>[]) for (const id of ids) expect(loc[id], id).toBeTruthy();
    expect(italian["proj.title"]).not.toBe(english["proj.title"]);
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
    expect(buildMenuActions(ctx(0))["tools.projecttexture"].enabled).toBe(false);
    const on = buildMenuActions(ctx(1));
    expect(on["tools.projecttexture"].enabled).toBe(true);
    on["tools.projecttexture"].run();
    expect(calls).toEqual(["projection"]);
  });
});
