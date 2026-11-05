import { useMemo, useState } from "react";
import { commands } from "../bindings";
import type {
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
  SpeechChoice,
} from "../bindings";
import { BROKEN, messageOf, testKey } from "../keyTest";
import { LanguagePicker } from "../LanguagePicker";
import { cloudLanguages } from "../languages";
import { megabytes } from "../settings/modelRow";
import { OwnKeyPanel, useOwnKey } from "../settings/ownKey";
import { spokenName, recommendedModel } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const MODEL_COPY = "Runs on this Mac. Works offline after a one-time download.";
export const KEY_REASON = "Continue unlocks after Test & Save succeeds";

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

  async function switchToCloud(which: Provider) {
    await commands.useCloudEngine(which);
    await reread();
  }

  const ownKey = useOwnKey({
    settings,
    onDevice: settings.transcription === "local",
    switchToCloud,
    refreshEngine,
  });

  const choice: Choice =
    picked ??
    (settings.transcription === "cloud" && ownKey.keyStatus.state === "saved" ? "key" : "local");

  const supported = useMemo(() => {
    const codes = new Set<string>(cloudLanguages(settings.provider));
    for (const model of models ?? []) {
      for (const code of model.language_codes) codes.add(code);
    }
    return [...codes];
  }, [models, settings.provider]);

  const model = models === null ? null : recommendedModel(models, language);

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

  async function continueWithKey() {
    // A key saved earlier hasn't been through Test & Save here: test it now.
    if (!ownKey.connected) {
      const outcome = await testKey(ownKey.provider, null);
      if (outcome.kind === "error") return outcome.text;
    }
    const used = await commands.useCloudEngine(ownKey.provider);
    if (used.status === "error") return messageOf(used.error);
    await makeRoomFor(null);
    await reread();
    await refreshEngine();
    return null;
  }

  async function go() {
    setWorking(true);
    setProblem(null);
    let failed: string | null;
    try {
      // The language on screen is the user's choice, even if they never
      // pressed it: English by default.
      if ((settings.language ?? "") !== language) {
        await commands.setLanguage(language === "" ? null : language);
      }
      if (choice === "key") failed = await continueWithKey();
      else failed = model === null ? null : await continueLocal(model);
    } catch {
      failed = BROKEN;
    }
    setWorking(false);
    if (failed !== null) {
      setProblem(failed);
      return;
    }
    onContinue();
  }

  const saved = ownKey.keyStatus.state === "saved";
  const blockedBecause = working
    ? "One moment…"
    : choice === "key"
      ? ownKey.connected || saved
        ? null
        : KEY_REASON
      : model === null
        ? models === null
          ? "Loading the models…"
          : "No on-device model speaks this language yet. Use your own key."
        : null;
  const needsDownload = choice === "local" && model !== null && model.state !== "ready";

  return (
    <StepFrame
      parts={parts}
      title="Which language will you mostly speak?"
      primary={{
        label: needsDownload ? "Download and continue" : "Continue",
        onClick: () => void go(),
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
              <span className="ob-card-title">
                {models === null ? "Loading the models…" : "No on-device model for this language"}
              </span>
            ) : (
              <span role="alert" className="error">
                Couldn&apos;t list the models: {modelsError}
              </span>
            )
          ) : (
            <>
              <span className="ob-card-title">
                {spokenName(model)} · {megabytes(model.size_bytes ?? 0)}
              </span>
              <span className="ob-caption">{MODEL_COPY}</span>
            </>
          )}
        </span>
      </label>

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
          <span className="ob-caption">
            Needs internet. Your speech is sent to the provider you choose.
          </span>
        </span>
      </label>
      {choice === "key" && <OwnKeyPanel ownKey={ownKey} />}

      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}
