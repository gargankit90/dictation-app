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

  it("styles app sheets with --vc-* variables only (no legacy names)", () => {
    for (const path of ["/src/settings.css", "/src/overlay/overlay.css"]) {
      const text = sources[path] ?? "";
      expect(text, path).not.toBe("");
      expect(text, path).not.toMatch(/var\(--(bg|fg|muted|border|accent|ok|danger)\b/);
      expect(text, path).not.toMatch(/rgba?\(/);
    }
  });
});
