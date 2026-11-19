// The few web pages the menus link to. In the desktop app the address is picked by name in Rust (open_link), never
// taken from the page; in a browser tab it opens as a new tab.
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./engine";

export const LINKS = {
  repo: "https://github.com/jasonsutter87/sketchcraft",
  license: "https://github.com/jasonsutter87/sketchcraft/blob/main/LICENSE",
  changelog: "https://github.com/jasonsutter87/sketchcraft/blob/main/CHANGELOG.md",
  issues: "https://github.com/jasonsutter87/sketchcraft/issues",
} as const;

export type LinkName = keyof typeof LINKS;

export function openLink(which: LinkName) {
  if (inTauri) void invoke("open_link", { which });
  else window.open(LINKS[which], "_blank", "noopener,noreferrer");
}
