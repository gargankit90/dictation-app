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

export const PROVIDERS: readonly ProviderInfo[] = [GROQ, OPENAI];

export function providerInfo(id: Provider): ProviderInfo {
  return id === "openai" ? OPENAI : GROQ;
}
