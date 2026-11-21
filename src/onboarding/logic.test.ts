import { describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize } from "../bindings";
import { chosenModel, modelWait, recommendedModel, stripView } from "./logic";

function model(update: Partial<LocalModelInfo_Serialize> = {}): LocalModelInfo_Serialize {
  return {
    id: "parakeet",
    name: "Parakeet Unified 0.6B",
    languages: "English only",
    language_codes: ["en"],
    size_bytes: 477_000_000,
    downloaded_bytes: 0,
    state: "missing",
    error: null,
    default: true,
    resumed_from_bytes: null,
    restarted: false,
    ...update,
  };
}

const QWEN = model({ id: "qwen", name: "Qwen3-ASR 0.6B", language_codes: ["en", "hi", "de"] });
const PARAKEET = model();

const SETTINGS = {
  version: 2,
  provider: "openrouter" as const,
  model: "openai/gpt-4o-mini-transcribe",
  hotkey: "RightAlt",
  launch_at_login: false,
  transcription: "local" as const,
  local_model: "parakeet",
  share_usage_events: true,
  history_limit: 25,
};

describe("recommendedModel", () => {
  it("is the English model, else the first", () => {
    expect(recommendedModel([QWEN, PARAKEET])?.id).toBe("parakeet");
    expect(recommendedModel([QWEN])?.id).toBe("qwen");
    expect(recommendedModel([])).toBeNull();
  });
});

describe("chosenModel", () => {
  it("is nothing while the engine is the own key", () => {
    expect(chosenModel({ ...SETTINGS, transcription: "cloud" }, [PARAKEET])).toBeNull();
    expect(chosenModel(SETTINGS, [PARAKEET])?.id).toBe("parakeet");
    expect(chosenModel(SETTINGS, null)).toBeNull();
  });
});

describe("stripView", () => {
  const running = model({ state: "downloading", downloaded_bytes: 212_000_000 });

  it("says the spec's sentence while downloading, with Cancel", () => {
    const strip = stripView([running], running);
    expect(strip?.text).toBe("Downloading on-device models (speech + cleanup) · 212 of 477 MB");
    expect(strip?.percent).toBe(44);
    expect(strip?.actions).toEqual(["cancel"]);
    expect(strip?.note).toBeNull();
  });

  it("claims a resume only when the server's answer backs it", () => {
    const kept = { ...running, resumed_from_bytes: 200_000_000 };
    expect(stripView([kept], kept)?.note).toBe("Continued from 200 MB.");
    // A restart that has overtaken the old part is still a restart.
    const restarted = { ...running, restarted: true };
    expect(stripView([restarted], restarted)?.note).toBe(
      "The earlier download couldn't continue, so it started again.",
    );
    // Nothing known yet: nothing claimed, whatever the bytes.
    expect(stripView([running], running)?.note).toBeNull();
  });

  it("offers Resume when paused and Retry when failed; nothing when ready", () => {
    const part = model({ state: "partial", downloaded_bytes: 212_000_000 });
    expect(stripView([part], part)?.text).toBe("Download paused · 212 of 477 MB");
    expect(stripView([part], part)?.actions).toEqual(["resume", "use-key"]);
    const failed = model({ state: "partial", error: "The disk is full." });
    expect(stripView([failed], failed)?.actions).toEqual(["retry", "use-key"]);
    const ready = model({ state: "ready" });
    expect(stripView([ready], ready)).toBeNull();
    // Own key chosen, nothing running: no strip.
    expect(stripView([part], null)).toBeNull();
  });
});

describe("modelWait", () => {
  const ready = model({ state: "ready" });
  const engine = { state: "ready" as const, message: null };

  it("never waits with the own key", () => {
    expect(modelWait({ ...SETTINGS, transcription: "cloud" }, [PARAKEET], null)).toBeNull();
  });
  it("gives the real percentage while downloading", () => {
    const halfway = model({ state: "downloading", downloaded_bytes: 390_000_000 });
    expect(modelWait(SETTINGS, [halfway], engine)).toBe("Getting the model ready: 82%");
  });
  it("waits for a part, a load and a failure, and not for a ready engine", () => {
    expect(modelWait(SETTINGS, [model({ state: "partial" })], engine)).toContain(
      "isn't downloaded",
    );
    expect(modelWait(SETTINGS, [ready], { state: "loading", message: null })).toContain("loading");
    expect(modelWait(SETTINGS, [ready], { state: "failed", message: "Bad file." })).toBe(
      "Bad file.",
    );
    expect(modelWait(SETTINGS, [ready], engine)).toBeNull();
  });
});
