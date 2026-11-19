import type { Provider } from "./bindings";

/** A provider the user can pick, with the name the UI shows. */
export interface ProviderInfo {
  readonly id: Provider;
  readonly name: string;
  /**
   * The model a new user gets (mirrors `Provider::default_model` in
   * `crates/core/src/settings.rs`; `commands::tests::the_ui_default_models_match_core`
   * keeps the two equal). Switching provider resets the model to this.
   */
  readonly defaultModel: string;
}

const GROQ: ProviderInfo = { id: "groq", name: "Groq", defaultModel: "whisper-large-v3-turbo" };
const OPENAI: ProviderInfo = { id: "openai", name: "OpenAI", defaultModel: "whisper-1" };

/** Experimental (exp/assemblyai): Vocet's own AssemblyAI account, reached with a token. */
// prettier-ignore
const ASSEMBLYAI: ProviderInfo = { id: "assemblyai", name: "Vocet Cloud (AssemblyAI)", defaultModel: "assemblyai-dictation" };

export const PROVIDERS: readonly ProviderInfo[] = [GROQ, OPENAI, ASSEMBLYAI];

export function providerInfo(id: Provider): ProviderInfo {
  return PROVIDERS.find((provider) => provider.id === id) ?? GROQ;
}
