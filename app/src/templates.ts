// Starter models, Canva-style. They are plain command scripts, the same
// format Claude and the CLI use.
import sunset from "../../examples/sunset_row_house.jsonl?raw";

const commands = (text: string) =>
  text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#"))
    .filter((l) => !/"cmd":"(render|export_obj|info)"/.test(l));

const all = commands(sunset);
const oneHouse = all.slice(0, all.findIndex((l) => l.includes('"cmd":"copy"')));

export const TEMPLATES = [
  {
    name: "Box",
    about: "A 6 × 8 m box, 3 m tall",
    tint: "#E9E6FF",
    lines: [
      '{"cmd":"begin_group","name":"Box"}',
      '{"cmd":"rect","from":[0,0,0],"to":[6,8,0],"as":"box_base"}',
      '{"cmd":"pushpull","face":"@box_base","distance":3}',
      '{"cmd":"end_group"}',
    ],
  },
  { name: "Row house", about: "One Sunset-style row house with stoop", tint: "#F6E7CF", lines: oneHouse },
  { name: "Sunset block", about: "Three row houses down a street", tint: "#DDEBF7", lines: all },
];
