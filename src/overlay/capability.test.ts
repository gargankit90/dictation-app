// M5.1 criterion 2: the overlay window's capability grants only what it
// uses (listening to the overlay event) and no commands. Any added
// permission, command, window or remote URL fails here, as does overlay code
// that starts calling commands.

import { describe, expect, it } from "vitest";

interface Capability {
  identifier?: unknown;
  windows?: unknown;
  webviews?: unknown;
  permissions?: unknown;
  remote?: unknown;
  local?: unknown;
  platforms?: unknown;
}

/** Every capability file, by file name. */
const capabilities: Record<string, Capability> = Object.fromEntries(
  Object.entries(
    import.meta.glob<Capability>("../../src-tauri/capabilities/*.json", {
      eager: true,
      import: "default",
    }),
  ).map(([path, capability]) => [path.split("/").pop() ?? path, capability]),
);

/** The overlay's own source files (not its tests), as text. */
const overlaySources: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob<string>(["./*.ts", "./*.tsx", "!./*.test.ts", "!./*.test.tsx"], {
      eager: true,
      query: "?raw",
      import: "default",
    }),
  ),
);

/** Listening to and unlistening from events: all the overlay may do. */
const OVERLAY_PERMISSIONS = ["core:event:allow-listen", "core:event:allow-unlisten"];

describe("criterion_m5_1_2 the overlay capability", () => {
  const overlay = capabilities["overlay.json"];

  it("exists", () => {
    expect(overlay).toBeDefined();
  });

  it("applies to the overlay window and nothing else", () => {
    expect(overlay?.identifier).toBe("overlay");
    expect(overlay?.windows).toEqual(["overlay"]);
    expect(overlay?.webviews).toBeUndefined();
    expect(overlay?.remote).toBeUndefined();
    expect(overlay?.local ?? true).toBe(true);
  });

  it("grants only listening to events: no commands, no other permission", () => {
    expect(overlay?.permissions).toEqual(OVERLAY_PERMISSIONS);
  });

  it("is the only capability that reaches the overlay window", () => {
    for (const [file, capability] of Object.entries(capabilities)) {
      if (file === "overlay.json") {
        continue;
      }
      const windows = Array.isArray(capability.windows) ? (capability.windows as unknown[]) : [];
      const webviews = Array.isArray(capability.webviews) ? (capability.webviews as unknown[]) : [];
      for (const pattern of [...windows, ...webviews]) {
        expect(typeof pattern, `${file}: ${String(pattern)}`).toBe("string");
        // A window pattern is a glob; anything with a wildcard could match
        // the overlay too.
        expect(pattern, file).not.toMatch(/[*?[]/);
        expect(pattern, file).not.toBe("overlay");
      }
    }
  });
});

describe("criterion_m5_1_2 the overlay code calls no commands", () => {
  it("has source files to check", () => {
    expect(Object.keys(overlaySources)).toContain("./Overlay.tsx");
  });

  it.each(Object.entries(overlaySources))("%s only listens", (_file, source) => {
    expect(source).not.toMatch(/@tauri-apps\/api\/core|invoke\(|\.\.\/bindings/);
    const tauriImports = [...source.matchAll(/from "(@tauri-apps\/[^"]+)"/g)].map((m) => m[1]);
    for (const module of tauriImports) {
      expect(module).toBe("@tauri-apps/api/event");
    }
  });
});
