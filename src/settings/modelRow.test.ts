import { describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize } from "../bindings";
import { languagesLabel, modelCaption, modelRow, modelTitle } from "./modelRow";
import type { RowContext } from "./modelRow";

const SIZE = 477_274_496;

function model(overrides: Partial<LocalModelInfo_Serialize> = {}): LocalModelInfo_Serialize {
  return {
    id: "parakeet",
    name: "Parakeet English",
    languages: "English only",
    language_codes: ["en"],
    size_bytes: SIZE,
    downloaded_bytes: 0,
    state: "missing",
    resumed_from_bytes: null,
    restarted: false,
    error: null,
    default: true,
    ...overrides,
  };
}

const IDLE: RowContext = {
  chosen: false,
  engine: { state: "ready", message: null },
  switching: false,
  loadError: null,
};

describe("criterion_u6_4 the model state machine", () => {
  it("not downloaded: the real size and Download", () => {
    const row = modelRow(model(), IDLE);
    expect(row.chip).toEqual({ text: "477 MB", tone: "plain" });
    expect(row.actions).toEqual(["download"]);
    expect(row.canDelete).toBe(false);
  });

  it("downloading: progress in MB and Cancel", () => {
    const row = modelRow(model({ state: "downloading", downloaded_bytes: 212_000_000 }), IDLE);
    expect(row.chip).toEqual({ text: "↓ 212 of 477 MB", tone: "info" });
    expect(row.actions).toEqual(["cancel"]);
    expect(row.progress).toEqual({ value: 212_000_000, max: SIZE });
  });

  it("paused without a connection: Retry", () => {
    const row = modelRow(
      model({
        state: "partial",
        downloaded_bytes: 10,
        error: "The download failed. Check your internet connection and try again.",
      }),
      IDLE,
    );
    expect(row.chip).toEqual({ text: "Paused: no connection", tone: "attention" });
    expect(row.actions).toEqual(["retry"]);
    expect(row.canDelete).toBe(true);
  });

  it("paused by Cancel: where it stopped, and Retry", () => {
    const row = modelRow(model({ state: "partial", downloaded_bytes: 212_000_000 }), IDLE);
    expect(row.chip.text).toBe("Download paused, 212 of 477 MB");
    expect(row.actions).toEqual(["resume"]);
  });

  it("failed: Download failed and Retry", () => {
    const failed = modelRow(model({ error: "Not enough disk space." }), IDLE);
    expect(failed.chip).toEqual({ text: "Download failed", tone: "error" });
    expect(failed.actions).toEqual(["retry"]);
    const damaged = modelRow(model({ state: "partial", error: "The file was damaged." }), IDLE);
    expect(damaged.chip.text).toBe("Download failed");
  });

  it("downloaded, not active: Use this, and Delete in the menu", () => {
    const row = modelRow(model({ state: "ready", downloaded_bytes: SIZE }), IDLE);
    expect(row.chip).toEqual({ text: "Downloaded", tone: "plain" });
    expect(row.actions).toEqual(["use"]);
    expect(row.canDelete).toBe(true);
    expect(row.active).toBe(false);
  });

  it("loading after Use this: no action", () => {
    const row = modelRow(model({ state: "ready" }), { ...IDLE, switching: true });
    expect(row.chip).toEqual({ text: "Loading…", tone: "info" });
    expect(row.actions).toEqual([]);
  });

  it("chosen and the engine still loading: Loading…", () => {
    const row = modelRow(model({ state: "ready" }), {
      ...IDLE,
      chosen: true,
      engine: { state: "loading", message: null },
    });
    expect(row.chip.text).toBe("Loading…");
  });

  it("active: ✓ Active in green, no action and no Delete", () => {
    const row = modelRow(model({ state: "ready" }), { ...IDLE, chosen: true });
    expect(row.chip).toEqual({ text: "✓ Active", tone: "ready" });
    expect(row.actions).toEqual([]);
    expect(row.canDelete).toBe(false);
    expect(row.active).toBe(true);
  });

  it("error after load: Couldn't load, Retry and Delete", () => {
    const chosen = modelRow(model({ state: "ready" }), {
      ...IDLE,
      chosen: true,
      engine: { state: "failed", message: "The model couldn't be loaded." },
    });
    expect(chosen.chip).toEqual({ text: "! Couldn't load", tone: "error" });
    expect(chosen.actions).toEqual(["retry-load", "delete"]);
    const tried = modelRow(model({ state: "ready" }), { ...IDLE, loadError: "Couldn't load it." });
    expect(tried.chip.text).toBe("! Couldn't load");
    expect(tried.actions).toEqual(["retry-load", "delete"]);
  });
});

describe("Design Lead round 1: model titles and captions", () => {
  it("titles are the language choice, the catalogue name goes in the caption", () => {
    expect(modelTitle(model())).toBe("English");
    expect(modelCaption(model())).toBe(
      "Parakeet English · English only · works offline · audio stays on this Mac",
    );
    const multi = model({ name: "Qwen3-ASR 0.6B", language_codes: ["en", "hi", "de", "fr"] });
    expect(modelTitle(multi)).toBe("Multilingual");
    expect(modelCaption(multi)).toBe(
      "Qwen3-ASR 0.6B · 4 languages, including English and Hindi · works offline · audio stays on this Mac",
    );
  });

  it("counts the languages from the codes, and never claims nothing is sent", () => {
    expect(languagesLabel(["de", "fr", "es"])).toBe("3 languages");
    expect(modelCaption(model())).not.toContain("nothing is sent");
  });
});
