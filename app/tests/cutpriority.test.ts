import { describe, expect, it } from "vitest";
import { moveTag, parsePriority, priorityCommand, priorityOf, priorityOrder, renumber, tiedTags } from "../src/cutpriority";
import type { TagStyleInfo } from "../src/engine";

const st = (p: number): TagStyleInfo => ({ cut: { priority: p } });

describe("priorityOf", () => {
  it("is 0 when unset, odd or out of range", () => {
    expect(priorityOf(undefined)).toBe(0);
    expect(priorityOf({})).toBe(0);
    expect(priorityOf({ cut: {} })).toBe(0);
    expect(priorityOf(st(7))).toBe(7);
    expect(priorityOf(st(1000))).toBe(0);
    expect(priorityOf(st(-1))).toBe(0);
    expect(priorityOf(st(1.5))).toBe(0);
  });
});

describe("priorityOrder", () => {
  it("lists the winner first and breaks ties by name, as the engine does", () => {
    const styles = { Walls: st(10), Slab: st(10), Insulation: st(50) };
    expect(priorityOrder(["Walls", "Slab", "Insulation", "Floor"], styles)).toEqual(["Insulation", "Slab", "Walls", "Floor"]);
    expect(priorityOrder(["b", "a"], undefined)).toEqual(["a", "b"]);
  });
});

describe("moveTag", () => {
  it("moves a tag up or down and clamps", () => {
    expect(moveTag(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
    expect(moveTag(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(moveTag(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"]);
    expect(moveTag(["a", "b", "c"], 1, 99)).toEqual(["a", "c", "b"]);
    expect(moveTag(["a", "b"], 5, 0)).toEqual(["a", "b"]);
  });
});

describe("renumber", () => {
  it("gives the bottom tag 0 and each one up 10 more, listing only what changes", () => {
    expect(renumber(["Walls", "Slab"], undefined)).toEqual({ Walls: 10 });
    expect(renumber(["Slab", "Walls"], { Walls: st(10) })).toEqual({ Slab: 10, Walls: 0 });
    expect(renumber(["a", "b", "c"], undefined)).toEqual({ a: 20, b: 10 });
    expect(renumber(["a", "b", "c"], { a: st(20), b: st(10) })).toEqual({});
    expect(renumber(["only"], undefined)).toEqual({});
  });

  it("keeps every number inside 0 to 999 with very many tags", () => {
    const tags = Array.from({ length: 800 }, (_, i) => `t${i}`);
    const got = renumber(tags, undefined);
    const all = tags.map((t) => got[t] ?? 0);
    expect(Math.max(...all)).toBeLessThanOrEqual(999);
    expect(all.every((v, i) => i === 0 || v < all[i - 1])).toBe(true);
  });

  it("makes the order true: reading it back gives the same list", () => {
    const order = ["c", "a", "d", "b"];
    const styles: Record<string, TagStyleInfo> = {};
    for (const [t, p] of Object.entries(renumber(order, undefined))) styles[t] = st(p);
    expect(priorityOrder(["a", "b", "c", "d"], styles)).toEqual(order);
  });
});

describe("parsePriority, priorityCommand, tiedTags", () => {
  it("accepts whole numbers 0 to 999 only", () => {
    expect(parsePriority(" 42 ")).toBe(42);
    expect(parsePriority("0")).toBe(0);
    expect(parsePriority("999")).toBe(999);
    for (const bad of ["", "1000", "-1", "2.5", "abc", "1e2"]) expect(parsePriority(bad)).toBeNull();
  });

  it("builds the tag_style command with only the priority", () => {
    expect(priorityCommand("Walls", 5)).toEqual({ cmd: "tag_style", tag: "Walls", cut: { priority: 5 } });
  });

  it("names the tags that share a number", () => {
    expect([...tiedTags(["a", "b", "c"], { a: st(5), b: st(5) })].sort()).toEqual(["a", "b"]);
    expect(tiedTags(["a", "b"], undefined).size).toBe(2);
    expect(tiedTags(["a", "b"], { a: st(1) }).size).toBe(0);
  });
});
