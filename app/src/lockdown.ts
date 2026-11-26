// The lockdown that runs first inside every plugin worker (JavaScript plugins
// and the SketchUp Ruby runtime): no network, no more code, and no way to
// start another worker (a new worker would begin without any of this).
// Kept as a string because it runs inside the worker; checks/run.mjs runs it
// against a pretend global to prove it does what it says.

export const LOCKDOWN_SOURCE = `
// In a browser these live on the prototypes (WorkerGlobalScope.prototype,
// WorkerNavigator.prototype), not on the global object itself, so hiding them on
// \`self\` alone leaves Object.getPrototypeOf(self).fetch reachable. Every name is
// closed on the object and on every prototype above it (up to Object.prototype).
const __block = (o, k) => {
  try { Object.defineProperty(o, k, { value: undefined, writable: false, configurable: false }); } catch {}
};
const __chain = (o, names) => {
  for (let p = o; p && p !== Object.prototype; p = Object.getPrototypeOf(p)) {
    for (const k of names) if (p === o || Object.prototype.hasOwnProperty.call(p, k)) __block(p, k);
  }
};
__chain(self, ["fetch", "XMLHttpRequest", "WebSocket", "WebSocketStream", "WebTransport", "EventSource", "importScripts", "indexedDB", "IDBFactory", "caches", "Worker", "SharedWorker", "BroadcastChannel", "Request", "RTCPeerConnection", "fetchLater", "createImageBitmap", "FontFace", "FontFaceSet", "Notification", "PushManager", "BackgroundFetchManager", "Cache", "CacheStorage", "FileSystemHandle", "FileSystemDirectoryHandle", "FileSystemFileHandle", "WebTransport", "RTCDataChannel", "RTCSessionDescription"]);
try { __block(self.navigator, "sendBeacon"); } catch {}
// storage a plugin could use to keep data beyond its run (and cache/streaming loads)
try { __chain(self.navigator, ["sendBeacon", "storage", "serviceWorker", "locks", "usb", "bluetooth", "hid", "serial", "gpu", "storageBuckets"]); } catch {}
for (const k of ["instantiateStreaming", "compileStreaming"]) {
  try { Object.defineProperty(self.WebAssembly, k, { value: undefined, configurable: false }); } catch {}
}
// Dynamic import() is part of the language and can't be removed: the window's
// content security policy stops it (and eval), and cspcheck.ts proves that holds before a plugin runs.
`;
