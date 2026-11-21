// Q4: "On this Mac" is one download (owner): the speech model and the
// cleanup model shown as one, with summed sizes and progress.

import { describe, expect, it } from "vitest";
import type { LocalModelInfo_Serialize } from "../bindings";
import { withCleanup } from "./CleanupModel";
import { downloadingText } from "./modelRow";

const SPEECH: LocalModelInfo_Serialize = {
  id: "orukqwen-asr-1.7b-q4_k_m",
  name: "OrukQwen 1.7B",
  languages: "English only",
  size_bytes: 1_319_830_656,
  downloaded_bytes: 0,
  state: "missing",
  resumed_from_bytes: null,
  restarted: false,
  error: null,
  default: true,
};

const GEMMA: LocalModelInfo_Serialize = {
  ...SPEECH,
  id: "gemma-4-e2b-it-q4_k_m",
  name: "Gemma 4 E2B",
  languages: "English",
  size_bytes: 3_106_738_272,
};

describe("withCleanup", () => {
  it("sums the sizes and keeps the speech model's id and languages", () => {
    const both = withCleanup(SPEECH, GEMMA);
    expect(both.id).toBe(SPEECH.id);
    expect(both.languages).toBe(SPEECH.languages);
    expect(both.size_bytes).toBe(4_426_568_928);
    expect(both.state).toBe("missing");
    expect(downloadingText(both)).toBe("Downloading on-device models (speech + cleanup) · 4.4 GB");
  });

  it("is one progress: the speech model's, then the cleanup model's on top", () => {
    const speech = withCleanup(
      { ...SPEECH, state: "downloading", downloaded_bytes: 500_000_000 },
      GEMMA,
    );
    expect(speech.state).toBe("downloading");
    expect(speech.downloaded_bytes).toBe(500_000_000);

    const cleanup = withCleanup(
      { ...SPEECH, state: "ready" },
      { ...GEMMA, state: "downloading", downloaded_bytes: 1_000_000_000, restarted: true },
    );
    expect(cleanup.state).toBe("downloading");
    expect(cleanup.downloaded_bytes).toBe(1_319_830_656 + 1_000_000_000);
    expect(cleanup.restarted).toBe(true);
  });

  it("is ready only when both are, and paused or failed when the cleanup model is", () => {
    expect(withCleanup({ ...SPEECH, state: "ready" }, { ...GEMMA, state: "ready" }).state).toBe(
      "ready",
    );
    expect(withCleanup({ ...SPEECH, state: "ready" }, { ...GEMMA, state: "partial" }).state).toBe(
      "partial",
    );
    const failed = withCleanup({ ...SPEECH, state: "ready" }, { ...GEMMA, error: "No space." });
    expect(failed.error).toBe("No space.");
  });

  it("without a cleanup model, it is the speech model", () => {
    expect(withCleanup(SPEECH, null)).toBe(SPEECH);
    expect(withCleanup(SPEECH, undefined)).toBe(SPEECH);
  });
});
