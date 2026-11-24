// Tools > Joinery and the CNC options of Cut plan: which boards a joint takes, the command each form sends, the words.
import { expect, it } from "vitest";
import { cncCommand, defaultToolText, jointBoards, jointCommand, jointSummary, JOINT_FIELDS } from "../src/joinery";
import english from "../src/locale-en";
import { MENUS, type MenuItem } from "../src/menumodel";
import { EMPTY } from "../src/selection";

const ids = (items: MenuItem[]): string[] => items.flatMap((i) => ("id" in i ? [i.id] : "items" in i ? ids(i.items) : []));

it("takes exactly two groups or components, and nothing else", () => {
  expect(jointBoards(EMPTY)).toBeNull();
  expect(jointBoards({ ...EMPTY, groups: [4] })).toBeNull();
  expect(jointBoards({ ...EMPTY, groups: [4, 5] })).toEqual([4, 5]);
  expect(jointBoards({ ...EMPTY, groups: [4], instances: [9] })).toEqual([4, 9]);
  expect(jointBoards({ ...EMPTY, groups: [4, 5, 6] })).toBeNull();
  expect(jointBoards({ ...EMPTY, groups: [4, 5], faces: [1] })).toBeNull();
});

it("sends the board to cut first, whichever was picked first", () => {
  const form = { cutFirst: true, depth: "", width: "", clearance: "" };
  expect(jointCommand("dado", [4, 5], form, "mm")).toEqual({ cmd: { cmd: "dado", ids: [4, 5] } });
  expect(jointCommand("dado", [4, 5], { ...form, cutFirst: false }, "mm")).toEqual({ cmd: { cmd: "dado", ids: [5, 4] } });
});

it("turns what was typed into meters, and leaves blanks to the engine", () => {
  const dado = jointCommand("dado", [1, 2], { cutFirst: true, depth: "6", width: "18.5", clearance: "0.2" }, "mm");
  expect(dado).toEqual({ cmd: { cmd: "dado", ids: [1, 2], depth: 0.006, width: 0.0185, clearance: 0.0002 } });
  const inch = jointCommand("rabbet", [1, 2], { cutFirst: true, depth: "1/4", width: "", clearance: "" }, "in");
  expect(inch).toEqual({ cmd: { cmd: "rabbet", ids: [1, 2], depth: 0.00635 } });
  const tenon = jointCommand("tenon", [1, 2], { cutFirst: true, length: "25", thickness: "6", shoulder: "0", clearance: "" }, "mm");
  expect(tenon).toEqual({ cmd: { cmd: "tenon", ids: [1, 2], length: 0.025, thickness: 0.006, shoulder: 0 } });
});

it("names the field that is not a length", () => {
  const f = { cutFirst: true, depth: "", width: "", clearance: "" };
  expect(jointCommand("dado", [1, 2], { ...f, depth: "deep" }, "mm")).toEqual({ error: "depth" });
  expect(jointCommand("dado", [1, 2], { ...f, depth: "0" }, "mm")).toEqual({ error: "depth" });
  expect(jointCommand("dado", [1, 2], { ...f, width: "-3" }, "mm")).toEqual({ error: "width" });
  // a clearance may be zero
  expect(jointCommand("dado", [1, 2], { ...f, clearance: "0" }, "mm")).toEqual({ cmd: { cmd: "dado", ids: [1, 2], clearance: 0 } });
});

it("asks each joint for its own fields", () => {
  expect(JOINT_FIELDS.tenon).toEqual(["length", "thickness", "shoulder", "clearance"]);
  expect(JOINT_FIELDS.dado).toEqual(JOINT_FIELDS.rabbet);
});

it("says how much a joint took out", () => {
  const t = (id: string) => (id === "joint.done.dado" ? "Dado: {cm3} cm³" : id);
  expect(jointSummary({ joint: "dado", removed_volume: 0.0000561 }, t)).toBe("Dado: 56.1 cm³");
});

it("asks for CNC files with the tool diameter in the units the form shows", () => {
  expect(defaultToolText("mm")).toBe("6");
  expect(defaultToolText("in")).toBe("0.25");
  expect(cncCommand(EMPTY, "/x/a.zip", "6", true, "mm")).toEqual({ cmd: { cmd: "export_cnc", path: "/x/a.zip", tool_diameter: 0.006, dogbones: true } });
  expect(cncCommand({ ...EMPTY, groups: [3], instances: [8] }, "/x/a.zip", "1/4", false, "in")).toEqual({ cmd: { cmd: "export_cnc", path: "/x/a.zip", ids: [3, 8], tool_diameter: 0.00635, dogbones: false } });
  for (const bad of ["", "big", "-2", "500"]) expect(cncCommand(EMPTY, "/x/a.zip", bad, false, "mm")).toEqual({ error: "tool" });
});

it("puts a Joinery submenu in Tools, and its messages exist", () => {
  const tools = MENUS.find((m) => m.id === "tools")!;
  expect(ids(tools.items)).toEqual(expect.arrayContaining(["tools.dado", "tools.rabbet", "tools.tenon"]));
  const keys = ["menu.joinery", "menu.dado", "menu.rabbet", "menu.tenon", "joint.cutWhich", "joint.pick", "cut.exportCnc", "cut.cnc.tool", "cut.cnc.dogbones", "cut.err.tool"];
  for (const kind of ["dado", "rabbet", "tenon"]) keys.push(`joint.title.${kind}`, `joint.says.${kind}`, `joint.apply.${kind}`, `joint.done.${kind}`);
  // (tests/locale.test.ts holds every other language to the English keys and placeholders)
  for (const k of keys) expect((english as Record<string, string>)[k], k).toBeTruthy();
});
