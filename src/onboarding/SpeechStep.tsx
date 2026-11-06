import { useEffect, useMemo, useRef, useState } from "react";
import { commands } from "../bindings";
import type {
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
  SpeechChoice,
} from "../bindings";
import { BROKEN, messageOf, testKey } from "../keyTest";
import { LanguagePicker } from "../LanguagePicker";
import { cloudLanguages, englishName } from "../languages";
import { modelLabel } from "../settings/modelRow";
import { OwnKeyPanel, useOwnKey } from "../settings/ownKey";
import type { OwnKey } from "../settings/ownKey";
import { recommendedModel } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const MODEL_COPY = "Runs on this Mac. Works offline after a one-time download.";
export const KEY_REASON = "Continue unlocks after Test & Save succeeds";
export const KEY_CARD_COPY =
  "About 100 languages. Needs internet. Your speech is sent to the provider you choose.";
export const KEY_SCREEN_COPY =
  "Vocet sends your speech to this provider, using your own account with them.";
export const KEY_SAVED = "✓ Key saved";
export const KEY_SAVED_COPY = "In your macOS Keychain. It's never shown again.";

/** The line for a language no on-device model covers. */
export function uncoveredCopy(code: string): string {
  return `${englishName(code)} needs your own key. Vocet's on-device models don't cover it yet.`;
}

/** How long to wait for a cancelled download to let go, before saying so. */
const CANCEL_TRIES = 15;
const CANCEL_WAIT_MS = 200;

interface Props {
  parts: FrameParts;
  /** The engine selected earlier, kept across a quit. */
  kept: SpeechChoice | null;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  models: LocalModelInfo_Serialize[] | null;
  modelsError: string | null;
  refreshModels: () => Promise<void>;
  refreshEngine: () => Promise<void>;
  onContinue: () => void;
}

type Choice = "local" | "key";

/** Waits while nothing is ready, so the step's own hooks start with settings. */
export function SpeechStep(props: Props) {
  if (props.settings === null) {
    return (
      <StepFrame
        parts={props.parts}
        title="Which language will you mostly speak?"
        primary={{ label: "Continue", onClick: () => undefined, blockedBecause: "Loading…" }}
      >
        <p>Loading…</p>
      </StepFrame>
    );
  }
  return <SpeechForm {...props} settings={props.settings} />;
}

function SpeechForm({
  parts,
  kept,
  settings,
  onStored,
  models,
  modelsError,
  refreshModels,
  refreshEngine,
  onContinue,
}: Props & { settings: Settings_Serialize }) {
  const [language, setLanguage] = useState(settings.language ?? "en");
  const [more, setMore] = useState(
    settings.language !== null && settings.language !== "en" && settings.language !== "hi",
  );
  // What the user picked here. Until they do: the recommendation, or the key
  // they already chose (coming back to the step).
  const [picked, setPicked] = useState<Choice | null>(
    kept === null ? null : kept === "own_key" ? "key" : "local",
  );

  function pick(choice: Choice) {
    setPicked(choice);
    setProblem(null);
    // The key itself isn't part of this: only which engine is selected.
    void commands.setSpeechChoice(choice === "key" ? "own_key" : "local").catch(() => undefined);
  }
  const [problem, setProblem] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  async function reread() {
    try {
      const result = await commands.getSettings();
      if (result.status === "ok") onStored(result.data);
    } catch {
      // What is shown stays; the next change reads again.
    }
  }

  // Setup changes the engine only on Continue, once the key has worked: changing
  // the Provider row here never switches it, whichever engine is in use now.
  const ownKey = useOwnKey({
    settings,
    onDevice: true,
    switchToCloud: () => Promise.resolve(),
    refreshEngine,
  });

  const [keyScreen, setKeyScreen] = useState(false);

  // The Continue running, as `keyRun` is for Test & Save in `ownKey.tsx`: anything
  // that changes what it was for makes it the old one, and an old one neither
  // switches the engine, nor moves on, nor reports. In the component: nothing can
  // come back to a step that's gone, so going away just drops it.
  const continueRun = useRef(0);
  useEffect(
    () => () => {
      continueRun.current += 1;
    },
    [],
  );
  function dropContinue() {
    continueRun.current += 1;
    setWorking(false);
  }

  const supported = useMemo(() => {
    const codes = new Set<string>(cloudLanguages(settings.provider));
    for (const model of models ?? []) {
      for (const code of model.language_codes) codes.add(code);
    }
    return [...codes];
  }, [models, settings.provider]);

  const model = models === null ? null : recommendedModel(models, language);
  // Known, not just not loaded yet: the key is then the only choice.
  const uncovered = models !== null && model === null;
  const choice: Choice = uncovered
    ? "key"
    : (picked ??
      (settings.transcription === "cloud" && ownKey.keyStatus.state === "saved" ? "key" : "local"));

  async function chooseLanguage(code: string) {
    setLanguage(code);
    setProblem(null);
    try {
      const result = await commands.setLanguage(code === "" ? null : code);
      if (result.status === "error") setProblem(messageOf(result.error));
    } catch {
      setProblem(BROKEN);
    }
    await reread();
  }

  /** Stops a download of another model: one model downloads at a time. */
  async function makeRoomFor(wanted: string | null) {
    const running = models?.find((item) => item.state === "downloading" && item.id !== wanted);
    if (running === undefined) return;
    await commands.cancelLocalModelDownload(running.id);
  }

  async function startDownload(id: string): Promise<string | null> {
    for (let attempt = 0; attempt < CANCEL_TRIES; attempt += 1) {
      const started = await commands.downloadLocalModel(id);
      if (started.status === "ok") return null;
      if (attempt === CANCEL_TRIES - 1) return messageOf(started.error);
      await new Promise((resolve) => setTimeout(resolve, CANCEL_WAIT_MS));
    }
    return null;
  }

  async function continueLocal(chosen: LocalModelInfo_Serialize) {
    const picked = await commands.chooseLocalModel(chosen.id);
    if (picked.status === "error") return messageOf(picked.error);
    await commands.setDownloadPaused(false);
    await makeRoomFor(chosen.id);
    if (chosen.state !== "ready") {
      const failed = await startDownload(chosen.id);
      if (failed !== null) return failed;
    }
    await reread();
    await refreshModels();
    await refreshEngine();
    return null;
  }

  async function continueWithKey(dropped: () => boolean) {
    // A key saved earlier hasn't been through Test & Save here: test it now.
    if (!ownKey.connected) {
      const outcome = await testKey(ownKey.provider, null);
      if (dropped()) return null;
      if (outcome.kind === "error") return outcome.text;
    }
    if (dropped()) return null;
    const used = await commands.useCloudEngine(ownKey.provider);
    if (dropped()) return null;
    if (used.status === "error") return messageOf(used.error);
    await makeRoomFor(null);
    await reread();
    await refreshEngine();
    return null;
  }

  async function go() {
    continueRun.current += 1;
    const run = continueRun.current;
    const dropped = () => run !== continueRun.current;
    setWorking(true);
    setProblem(null);
    let failed: string | null;
    try {
      // The language on screen is the user's choice, even if they never
      // pressed it: English by default.
      if ((settings.language ?? "") !== language) {
        await commands.setLanguage(language === "" ? null : language);
      }
      if (choice === "key") failed = await continueWithKey(dropped);
      else failed = model === null ? null : await continueLocal(model);
    } catch {
      failed = BROKEN;
    }
    // The screen it was for has moved on: whatever came back no longer counts.
    if (dropped()) return;
    setWorking(false);
    if (failed !== null) {
      setProblem(failed);
      return;
    }
    onContinue();
  }

  const blockedBecause = working
    ? "One moment…"
    : choice === "key"
      ? null
      : model === null
        ? "Loading the models…"
        : null;
  const needsDownload = choice === "local" && model !== null && model.state !== "ready";

  function backFromKey() {
    dropContinue();
    ownKey.abandon();
    setProblem(null);
    setKeyScreen(false);
  }

  if (keyScreen) {
    return (
      <KeyScreen
        parts={parts}
        ownKey={{
          ...ownKey,
          changeProvider: (next) => {
            dropContinue();
            ownKey.changeProvider(next);
          },
          editKey: (value) => {
            dropContinue();
            ownKey.editKey(value);
          },
          testAndSave: () => {
            dropContinue();
            return ownKey.testAndSave();
          },
        }}
        problem={problem}
        working={working}
        onBack={backFromKey}
        onContinue={() => void go()}
      />
    );
  }

  return (
    <StepFrame
      parts={parts}
      title="Which language will you mostly speak?"
      primary={{
        label: needsDownload ? "Download and continue" : "Continue",
        onClick: () => {
          if (choice === "key") {
            // The key itself isn't part of this: only which engine is selected.
            void commands.setSpeechChoice("own_key").catch(() => undefined);
            setProblem(null);
            setKeyScreen(true);
          } else {
            void go();
          }
        },
        blockedBecause,
      }}
    >
      <div className="ob-languages" role="radiogroup" aria-label="Language">
        {(
          [
            ["en", "English"],
            ["hi", "Hindi"],
          ] as const
        ).map(([code, name]) => (
          <button
            key={code}
            type="button"
            className="ob-chip"
            role="radio"
            aria-checked={!more && language === code}
            onClick={() => {
              setMore(false);
              void chooseLanguage(code);
            }}
          >
            {name}
          </button>
        ))}
        <button
          type="button"
          className="ob-chip"
          role="radio"
          aria-checked={more}
          onClick={() => {
            setMore(true);
          }}
        >
          More…
        </button>
      </div>
      {more && (
        <LanguagePicker
          id="ob-language"
          value={language}
          onChange={(code) => void chooseLanguage(code)}
          supported={supported}
          engineName={null}
        />
      )}

      {uncovered ? (
        <p role="status" className="ob-sub">
          {uncoveredCopy(language)}
        </p>
      ) : (
        <label className={`ob-card ob-option${choice === "local" ? " selected" : ""}`}>
          <input
            type="radio"
            name="ob-engine"
            aria-label="Recommended for your language"
            checked={choice === "local"}
            disabled={model === null}
            onChange={() => {
              pick("local");
            }}
          />
          <span className="ob-option-text">
            <span className="ob-eyebrow">Recommended for your language</span>
            {model === null ? (
              modelsError === null ? (
                <span className="ob-card-title">Loading the models…</span>
              ) : (
                <span role="alert" className="error">
                  Couldn&apos;t list the models: {modelsError}
                </span>
              )
            ) : (
              <>
                <span className="ob-card-title">{modelLabel(model)}</span>
                <span className="ob-caption">{MODEL_COPY}</span>
              </>
            )}
          </span>
        </label>
      )}

      <label className={`ob-card ob-option${choice === "key" ? " selected" : ""}`}>
        <input
          type="radio"
          name="ob-engine"
          aria-label="Use my own Groq or OpenAI key"
          checked={choice === "key"}
          onChange={() => {
            pick("key");
          }}
        />
        <span className="ob-option-text">
          <span className="ob-card-title">Use my own Groq or OpenAI key</span>
          <span className="ob-caption">{KEY_CARD_COPY}</span>
        </span>
      </label>

      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}

/**
 * Connect your key: its own screen under Speech, so the rail doesn't move (U11).
 * The form is Settings' own (`ownKey.tsx`), in a page and without Remove key. A
 * saved key shows no field until Replace. Whichever key Continue would use has
 * either just passed Test & Save or is tested first, and a failure stays here.
 */
function KeyScreen({
  parts,
  ownKey,
  problem,
  working,
  onBack,
  onContinue,
}: {
  parts: FrameParts;
  ownKey: OwnKey;
  problem: string | null;
  working: boolean;
  onBack: () => void;
  onContinue: () => void;
}) {
  const { provider, keyInput, keyStatus, keyBusy, connected } = ownKey;
  // For one provider: another one starts from what it has saved.
  const [replacing, setReplacing] = useState<Provider | null>(null);
  const saved = keyStatus.state === "saved";
  // A Test & Save that worked is the end of replacing.
  const showField = !saved || (replacing === provider && !connected);
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (keyStatus.state === "checking" || !showField) return;
    panel.current?.querySelector<HTMLInputElement>("#api-key")?.focus();
  }, [keyStatus.state, showField, provider]);

  const checking = keyStatus.state === "checking";
  // What's typed isn't saved: Continue would quietly use the old key.
  const blockedBecause = working
    ? "One moment…"
    : checking
      ? "Checking for a saved key…"
      : saved && keyInput.trim() === ""
        ? null
        : KEY_REASON;

  return (
    <StepFrame
      parts={{
        ...parts,
        back: onBack,
        // Once the key is being written nothing leaves, as in Settings' sheet.
        exitBlock: parts.exitBlock ?? (keyBusy === "saving" ? "Saving your key…" : null),
      }}
      title="Connect your key"
      primary={{ label: "Continue", onClick: onContinue, blockedBecause }}
    >
      <p className="ob-sub">{KEY_SCREEN_COPY}</p>
      <div className="ob-key-panel" ref={panel}>
        <OwnKeyPanel ownKey={ownKey} canRemove={false} providerOnly={checking || !showField} />
      </div>
      {saved && !showField && (
        <div className="ob-key-saved">
          <div className="ob-card-text">
            <span role="status" className="ob-key-saved-title">
              {KEY_SAVED}
            </span>
            <span className="ob-caption">{KEY_SAVED_COPY}</span>
          </div>
          <button
            type="button"
            onClick={() => {
              // Clears the last result, which is what ended the replacing before.
              ownKey.editKey("");
              setReplacing(provider);
            }}
          >
            Replace
          </button>
        </div>
      )}
      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}
