// Name shims for `require` in the Ruby worker (and the Node test harness): the plugin's own files are found by
// name, and SketchUp's and Ruby's library names that SketchCraft provides itself are already loaded.
export const RUBY_SHIMS = `
for (const n of ["sketchup", "sketchup.rb", "extensions", "extensions.rb", "langhandler", "langhandler.rb", "json", "set", "fileutils", "tempfile", "date", "time", "uri", "digest", "securerandom", "ostruct", "cgi", "net/http", "net/https", "open-uri", "base64", "zlib", "stringio", "benchmark", "logger", "singleton", "forwardable", "observer", "timeout", "thread", "monitor", "pathname", "rexml/document", "csv", "yaml", "erb", "win32ole", "socket"]) Opal.modules[n] = function () {};
(function () {
  const req = Opal.require;
  Opal.require = function (name) {
    let n = String(name).replace(/\\.(rb|rbe|rbs)$/i, "").replace(/^\\.\\//, "");
    if (Opal.modules[n]) return req.call(Opal, n);
    // relative to a plugin folder: try dropping leading parts
    const parts = n.split("/");
    for (let i = 1; i < parts.length; i++) { const m = parts.slice(i).join("/"); if (Opal.modules[m]) return req.call(Opal, m); }
    for (const m of Object.keys(Opal.modules)) if (m.endsWith("/" + n)) return req.call(Opal, m);
    return req.call(Opal, n);
  };
})();`;
