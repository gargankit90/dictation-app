import type { Provider } from "./bindings";

/** The cloud provider, with the name the UI shows. */
export interface ProviderInfo {
  readonly id: Provider;
  readonly name: string;
  /**
   * The speech model a new user gets (mirrors `Provider::default_model` in
   * `crates/core/src/settings.rs`; `commands::tests::the_ui_default_models_match_core`
   * keeps the two equal).
   */
  readonly defaultModel: string;
  /** Display names from the catalog in `crates/core/src/cloud_models.rs` (same test). */
  readonly speechModelName: string;
  readonly cleanupModelName: string;
}

const OPENROUTER: ProviderInfo = {
  id: "openrouter",
  name: "OpenRouter",
  defaultModel: "openai/gpt-4o-mini-transcribe",
  speechModelName: "GPT-4o mini Transcribe (OpenAI)",
  cleanupModelName: "GLM 5.3 Flash (Z.ai)",
};

export const PROVIDERS: readonly ProviderInfo[] = [OPENROUTER];

export function providerInfo(): ProviderInfo {
  return OPENROUTER;
}
