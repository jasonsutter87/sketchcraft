import { describe, expect, it } from "vitest";
import filePaths from "../src-tauri/gen/android/app/src/main/res/xml/file_paths.xml?raw";
import manifest from "../src-tauri/gen/android/app/src/main/AndroidManifest.xml?raw";

const read = (p: string) => (p.endsWith("file_paths.xml") ? filePaths : manifest);

describe("Android file sharing", () => {
  it("the FileProvider shares only a folder in the app's own cache", () => {
    const paths = read("res/xml/file_paths.xml").replace(/<!--[\s\S]*?-->/g, "");
    expect(paths).not.toMatch(/external-path|external-files-path|external-media-path|root-path|files-path/);
    expect(paths).not.toMatch(/path="\.?\/?"/);
    expect(paths).toMatch(/<cache-path [^>]*path="shared\/"/);
  });

  it("the provider is not exported, and cleartext stays a build setting", () => {
    const m = read("AndroidManifest.xml");
    const provider = m.slice(m.indexOf("<provider"), m.indexOf("</provider>"));
    expect(provider).toContain('android:exported="false"');
    expect(m).toContain('android:usesCleartextTraffic="${usesCleartextTraffic}"');
  });
});
