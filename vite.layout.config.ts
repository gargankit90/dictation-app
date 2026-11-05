// Builds the onboarding layout test page (src/onboarding/layoutHarness.tsx) as
// one classic script and one stylesheet, which Safari will run from a file://
// URL. Never part of the app's bundle: `vite build` uses vite.config.ts.
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    target: "safari16",
    outDir: "tests/e2e/out/onboarding-layout",
    emptyOutDir: true,
    cssCodeSplit: false,
    lib: {
      entry: "src/onboarding/layoutHarness.tsx",
      name: "layoutHarness",
      formats: ["iife"],
      fileName: () => "harness.js",
      cssFileName: "harness",
    },
  },
});
