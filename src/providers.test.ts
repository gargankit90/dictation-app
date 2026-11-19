import { describe, expect, it } from "vitest";
import { cloudLanguages } from "./languages";
import { PROVIDERS, providerInfo } from "./providers";

describe("providers", () => {
  it("offers Vocet Cloud (AssemblyAI) next to Groq and OpenAI", () => {
    expect(PROVIDERS.map((p) => p.id)).toEqual(["groq", "openai", "assemblyai"]);
    expect(providerInfo("assemblyai").name).toBe("Vocet Cloud (AssemblyAI)");
    expect(providerInfo("openai").name).toBe("OpenAI");
  });

  it("limits its languages to the ones the Dictation API accepts", () => {
    const languages = cloudLanguages("assemblyai");
    expect(languages).toHaveLength(32);
    expect(languages).toContain("en");
    expect(languages).not.toContain("th");
  });
});
