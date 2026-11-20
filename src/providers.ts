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
}

const OPENROUTER: ProviderInfo = {
  id: "openrouter",
  name: "OpenRouter",
  defaultModel: "openai/gpt-4o-mini-transcribe",
};

export const PROVIDERS: readonly ProviderInfo[] = [OPENROUTER];

export function providerInfo(): ProviderInfo {
  return OPENROUTER;
}
