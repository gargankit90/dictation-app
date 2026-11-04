import { describe, expect, it } from "vitest";
import type { DictationProgressView, LocalModelInfo_Serialize } from "../bindings";
import {
  NO_SOUND,
  NO_WORDS,
  chosenModel,
  holdsWords,
  modelWait,
  recommendedModel,
  spokenName,
  stripView,
  tryView,
} from "./logic";

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
    ...update,
  };
}

const QWEN = model({ id: "qwen", name: "Qwen3-ASR 0.6B", language_codes: ["en", "hi", "de"] });
const PARAKEET = model();

const BASE: DictationProgressView = {
  started: 2,
  finished: 2,
  completed: 1,
  errors: 0,
  stages: [],
  error: null,
  failure: null,
  heard: false,
};

const SETTINGS = {
  version: 2,
  provider: "groq" as const,
  model: "whisper-large-v3-turbo",
  language: null,
  hotkey: "RightAlt",
  launch_at_login: false,
  transcription: "local" as const,
  local_model: "parakeet",
  share_usage_events: true,
};

describe("recommendedModel", () => {
  it("is the English model for English", () => {
    expect(recommendedModel([QWEN, PARAKEET], "en")?.id).toBe("parakeet");
  });
  it("is the multilingual model for Hindi, and for automatic", () => {
    expect(recommendedModel([PARAKEET, QWEN], "hi")?.id).toBe("qwen");
    expect(recommendedModel([PARAKEET, QWEN], "")?.id).toBe("qwen");
  });
  it("is nothing for a language no model speaks", () => {
    expect(recommendedModel([PARAKEET, QWEN], "xx")).toBeNull();
  });
});

describe("spokenName", () => {
  it("names the language choice", () => {
    expect(spokenName(PARAKEET)).toBe("Parakeet English");
    expect(spokenName(QWEN)).toBe("Qwen3-ASR Multilingual");
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
    const strip = stripView([running], running, null);
    expect(strip?.text).toBe("Downloading Parakeet English · 212 of 477 MB");
    expect(strip?.percent).toBe(44);
    expect(strip?.actions).toEqual(["cancel"]);
    expect(strip?.note).toBeNull();
  });

  it("claims a resume only when the bytes back it", () => {
    expect(stripView([running], running, 200_000_000)?.note).toBe("Continued from 200 MB.");
    expect(stripView([running], running, 300_000_000)?.note).toBe(
      "The earlier download couldn't continue, so it started again.",
    );
    // No bytes reported yet: nothing to claim.
    const starting = model({ state: "downloading", downloaded_bytes: 0 });
    expect(stripView([starting], starting, 200_000_000)?.note).toBeNull();
  });

  it("offers Resume when paused and Retry when failed; nothing when ready", () => {
    const part = model({ state: "partial", downloaded_bytes: 212_000_000 });
    expect(stripView([part], part, null)?.text).toBe("Download paused · 212 of 477 MB");
    expect(stripView([part], part, null)?.actions).toEqual(["resume", "use-key"]);
    const failed = model({ state: "partial", error: "The disk is full." });
    expect(stripView([failed], failed, null)?.actions).toEqual(["retry", "use-key"]);
    const ready = model({ state: "ready" });
    expect(stripView([ready], ready, null)).toBeNull();
    // Own key chosen, nothing running: no strip.
    expect(stripView([part], null, null)).toBeNull();
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

describe("tryView", () => {
  const started = { ...BASE, started: 3, stages: ["recorded" as const] };

  it("claims nothing without a dictation since the step opened", () => {
    expect(tryView(null, BASE, "landed").pasted).toBe(false);
    expect(tryView(BASE, BASE, "landed").pasted).toBe(false);
    expect(tryView({ ...BASE, heard: true }, BASE, "unknown").heard).toBe(false);
  });

  it("each stage comes from its own report", () => {
    expect(tryView(started, BASE, "unknown").heard).toBe(false);
    expect(tryView({ ...started, heard: true }, BASE, "unknown").heard).toBe(true);
    expect(tryView(started, BASE, "unknown").transcribed).toBe(false);
    const transcribed = { ...started, stages: ["recorded" as const, "transcribed" as const] };
    expect(tryView(transcribed, BASE, "unknown").transcribed).toBe(true);
    expect(tryView(transcribed, BASE, "unknown").pasted).toBe(false);
    expect(tryView(transcribed, BASE, "landed").pasted).toBe(true);
  });

  it("a paste that missed asks for the paste fix", () => {
    expect(tryView(started, BASE, "missed").fix).toEqual({ kind: "paste" });
  });

  it("over with nothing transcribed: no sound, or no words", () => {
    const over = { ...started, finished: 3 };
    expect(tryView(over, BASE, "unknown").fix).toEqual({ kind: "microphone", message: NO_SOUND });
    expect(tryView({ ...over, heard: true }, BASE, "unknown").fix).toEqual({
      kind: "other",
      message: NO_WORDS,
    });
    // Still running: no verdict yet.
    expect(tryView(started, BASE, "unknown").fix).toEqual({ kind: "none" });
  });

  it("an error names its one fix", () => {
    const failed = { ...started, error: "Sentence." };
    expect(tryView({ ...failed, failure: "key" }, BASE, "unknown").fix.kind).toBe("key");
    expect(tryView({ ...failed, failure: "microphone" }, BASE, "unknown").fix.kind).toBe(
      "microphone",
    );
    expect(tryView({ ...failed, failure: "paste" }, BASE, "unknown").fix.kind).toBe("paste");
    expect(tryView({ ...failed, failure: "other" }, BASE, "unknown").fix).toEqual({
      kind: "other",
      message: "Sentence.",
    });
  });
});

describe("holdsWords", () => {
  it("finds the words among others, whatever the spacing", () => {
    expect(holdsWords("before  hello\nthere after", "hello there")).toBe(true);
    expect(holdsWords("something else", "hello there")).toBe(false);
    expect(holdsWords("anything", "   ")).toBe(false);
  });
});
