import { describe, expect, it } from "vitest";

/** Files allowed to contain a hex colour, with the reason. The single list. */
const HEX_EXCEPTIONS: Record<string, string> = {
  "/src/styles/tokens.css": "the token definitions themselves (DS1 copy)",
};

const sources = import.meta.glob<string>(
  ["/src/**/*.{css,ts,tsx}", "!/src/**/*.test.{ts,tsx}", "!/src/bindings.ts"],
  { query: "?raw", import: "default", eager: true },
);

const site = import.meta.glob<string>("/site/index.html", {
  query: "?raw",
  import: "default",
  eager: true,
});
const fontFiles = import.meta.glob("/site/fonts/*.woff2");
const fontLicence = import.meta.glob<string>("/site/fonts/LICENSE.txt", {
  query: "?raw",
  import: "default",
  eager: true,
});

// In stylesheets any #rgb/#rrggbb/#rrggbbaa; in TS only quoted colour literals.
const CSS_HEX = /#[0-9a-fA-F]{3,8}\b/;
const TS_HEX = /["'`]#[0-9a-fA-F]{3,8}["'`]/;

describe("design tokens", () => {
  it("copies DS1 tokens with the source header", () => {
    const css = sources["/src/styles/tokens.css"] ?? "";
    expect(css.split("\n")[0]).toContain("Source: design-system/tokens.css, DS1");
    expect(css).toContain("--vc-accent:");
    expect(css).toContain("prefers-color-scheme: dark");
    expect(css).toContain("prefers-reduced-motion: reduce");
  });

  it("imports tokens.css from both entry points", () => {
    expect(sources["/src/main.tsx"]).toContain('"./styles/tokens.css"');
    expect(sources["/src/overlay/main.tsx"]).toContain('"../styles/tokens.css"');
  });

  it("uses no hex colour outside tokens.css", () => {
    const offenders = Object.entries(sources)
      .filter(([path]) => !(path in HEX_EXCEPTIONS))
      .filter(([path, text]) => (path.endsWith(".css") ? CSS_HEX : TS_HEX).test(text))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it("keeps the website on the same tokens, with fonts served from site/fonts", () => {
    const html = site["/site/index.html"] ?? "";
    expect(html).toMatch(/<html[^>]*class="vc-site"/);
    expect(html).toContain("Source: design-system/tokens.css, DS1");
    expect(html).toContain("--accent: var(--vc-accent)");
    // No third-party requests except the release lookup.
    expect(html).not.toMatch(/fonts\.(googleapis|gstatic)\.com/);
    const hosts = [
      ...html.matchAll(/<(?:link|script|img|iframe)\b[^>]*\b(?:src|href)="(https?:\/\/[^"/]+)/g),
    ].map((m) => m[1] ?? "");
    expect(hosts.filter((h) => !h.includes("vocetlabs.com"))).toEqual([]);
    // Every self-hosted font file the CSS names exists, and the OFL is credited.
    const files = [...html.matchAll(/url\("fonts\/([^"]+)"\)/g)].map(
      (m) => `/site/fonts/${m[1] ?? ""}`,
    );
    expect(files.length).toBeGreaterThan(0);
    for (const f of files) expect(Object.keys(fontFiles), f).toContain(f);
    expect(html).toMatch(/font-display:\s*swap/);
    expect(fontLicence["/site/fonts/LICENSE.txt"]).toContain("SIL OPEN FONT LICENSE");
  });

  // U12 criterion 8, from the Design Lead.
  describe("calmer fills", () => {
    const css = sources["/src/styles/tokens.css"] ?? "";
    const darkAt = css.indexOf("prefers-color-scheme: dark");
    const value = (name: string, dark: boolean): string => {
      const part = dark ? css.slice(darkAt) : css.slice(0, darkAt);
      const found = new RegExp(`--vc-${name}:\\s*(#[0-9a-f]{6})`).exec(part);
      if (found?.[1] === undefined) throw new Error(`no ${name}`);
      return found[1];
    };
    const luminance = (hex: string): number => {
      const [r = 0, g = 0, b = 0] = [1, 3, 5].map((at) => {
        const channel = parseInt(hex.slice(at, at + 2), 16) / 255;
        return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const contrast = (a: string, b: string): number => {
      const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return ((high ?? 0) + 0.05) / ((low ?? 0) + 0.05);
    };

    it("has the Design Lead's values; light is unchanged", () => {
      expect(value("primary-bg", false)).toBe(value("accent", false));
      expect(value("primary-hover", false)).toBe(value("accent-hover", false));
      expect(value("primary-fg", false)).toBe("#ffffff");
      expect(value("selection-bg", false)).toBe(value("accent", false));
      expect(value("primary-bg", true)).toBe("#3b6be0");
      expect(value("primary-hover", true)).toBe("#3461cf");
      expect(value("primary-fg", true)).toBe("#ffffff");
      // Setup's navy, the fill of its rail's current step.
      expect(value("selection-bg", true)).toBe(value("accent-soft", true));
    });

    it("passes contrast in both appearances", () => {
      for (const dark of [false, true]) {
        for (const [fill, text] of [
          ["primary-bg", "primary-fg"],
          ["primary-hover", "primary-fg"],
          ["selection-bg", "selection-fg"],
        ] as const) {
          expect(contrast(value(fill, dark), value(text, dark)), fill).toBeGreaterThanOrEqual(4.5);
        }
      }
    });

    it("is what the primary button, the switch and the sidebar use", () => {
      const sheet = sources["/src/settings.css"] ?? "";
      expect(sheet).toMatch(/button\.primary \{[^}]*background: var\(--vc-primary-bg\)/);
      expect(sheet).toMatch(/input\.switch:checked \{[^}]*background: var\(--vc-primary-bg\)/);
      expect(sheet).toMatch(
        /\.settings-sidebar button\.selected \{[^}]*background: var\(--vc-selection-bg\)/,
      );
    });

    // U12 criterion 9.
    it("draws the sidebar's focus ring inside the row", () => {
      const sheet = sources["/src/settings.css"] ?? "";
      expect(sheet).toMatch(
        /\.settings-sidebar button\.selected:focus-visible \{[^}]*outline-offset: -4px/,
      );
      expect(sheet).toMatch(/\.settings-sidebar button:focus-visible \{[^}]*outline-offset: -2px/);
    });
  });

  it("styles app sheets with --vc-* variables only (no legacy names)", () => {
    for (const path of ["/src/settings.css", "/src/overlay/overlay.css"]) {
      const text = sources[path] ?? "";
      expect(text, path).not.toBe("");
      expect(text, path).not.toMatch(/var\(--(bg|fg|muted|border|accent|ok|danger)\b/);
      expect(text, path).not.toMatch(/rgba?\(/);
    }
  });
});
