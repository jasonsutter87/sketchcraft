// Your 3D mouse settings, live (the same shape as keymapstore.ts): read when the app starts, kept in memory, saved when
// they change. Also the real transport: in the desktop app the shell reads the device and sends frames as Tauri
// events; in a browser there is no device, and everything here stays quiet.

import { useEffect, useRef, useSyncExternalStore, type RefObject } from "react";
import type { ViewportApi } from "./viewport/types";
import { store } from "./skins";
import { inTauri } from "./engine";
import { NO_DEVICE, SpaceMouseDriver, SPACEMOUSE_KEY, loadSettings, readFrame, type SpaceMouseFrame, type SpaceMouseSettings, type SpaceMouseStatus, type SpaceMouseTransport } from "./spacemouse";

let settings: SpaceMouseSettings = loadSettings();
let status: SpaceMouseStatus = NO_DEVICE;
const listeners = new Set<() => void>();
const publish = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => (listeners.add(l), () => void listeners.delete(l));

export const spaceMouse = {
  settings: () => settings,
  status: () => status,
  /** change a setting (saved straight away) */
  set(next: SpaceMouseSettings) {
    settings = next;
    store.set(SPACEMOUSE_KEY, JSON.stringify(next));
    publish();
  },
  setStatus(s: SpaceMouseStatus) {
    status = s;
    publish();
  },
  subscribe,
};

/** Re-render when the settings or the device's state change. */
export function useSpaceMouse() {
  const s = useSyncExternalStore(subscribe, () => settings);
  const d = useSyncExternalStore(subscribe, () => status);
  return { settings: s, status: d };
}

/** The desktop shell's transport (events from app/src-tauri/src/spacemouse.rs), or null where there is no shell. */
export async function tauriTransport(): Promise<SpaceMouseTransport | null> {
  if (!inTauri) return null;
  const [{ listen }, { invoke }] = await Promise.all([import("@tauri-apps/api/event"), import("@tauri-apps/api/core")]);
  const on = (event: string, cb: (v: unknown) => void) => {
    const p = listen<unknown>(event, (e) => cb(e.payload));
    return () => void p.then((un) => un());
  };
  const readStatus = (raw: unknown): SpaceMouseStatus => {
    const o = (raw ?? {}) as Record<string, unknown>;
    return { connected: o.connected === true, name: typeof o.name === "string" ? o.name.slice(0, 80) : "", via: typeof o.via === "string" ? o.via.slice(0, 20) : "" };
  };
  return {
    enable: (v) => void invoke("spacemouse_enable", { on: v }).catch(() => {}),
    onFrame: (cb: (f: SpaceMouseFrame) => void) => on("spacemouse", (p) => cb(readFrame(p))),
    onStatus: (cb: (s: SpaceMouseStatus) => void) => {
      void invoke("spacemouse_status").then((s) => cb(readStatus(s)), () => {});
      return on("spacemouse-status", (p) => cb(readStatus(p)));
    },
  };
}

/** Connects a 3D mouse (when the desktop shell has one) to the view. Nothing happens in a browser, or with no device. */
export function useSpaceMouseNav(api: RefObject<ViewportApi | null>) {
  const driver = useRef<SpaceMouseDriver | null>(null);
  useEffect(() => {
    let gone = false;
    let stop: (() => void)[] = [];
    void tauriTransport().then((transport) => {
      if (!transport || gone) return;
      stop = [transport.onStatus((s) => spaceMouse.setStatus(s))];
      const d = new SpaceMouseDriver(transport, {
        apply: (n) => api.current?.navigate(n),
        button: (a) => (a === "fit" ? api.current?.fit() : api.current?.view("iso")),
        schedule: (fn) => {
          const id = requestAnimationFrame(fn);
          return () => cancelAnimationFrame(id);
        },
        settings: spaceMouse.settings,
      });
      driver.current = d;
      stop.push(() => d.dispose());
    });
    return () => {
      gone = true;
      stop.forEach((f) => f());
      driver.current = null;
    };
  }, [api]);
  // turning it on or off in the settings reaches the shell (and lets go of the cap)
  const { settings } = useSpaceMouse();
  useEffect(() => {
    driver.current?.sync();
  }, [settings.enabled]);
}
