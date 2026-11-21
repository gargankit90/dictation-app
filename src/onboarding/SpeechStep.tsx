import { useEffect, useRef, useState } from "react";
import { commands } from "../bindings";
import type { LocalModelInfo_Serialize, Settings_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../messages";
import { modelLabel } from "../settings/modelRow";
import { ConnectPanel, useConnection } from "../settings/connection";
import type { Connection } from "../settings/connection";
import { recommendedModel } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const MODEL_COPY = "Runs on this Mac. Works offline after a one-time download.";
export const KEY_REASON = "Continue unlocks once OpenRouter is connected";
export const KEY_CARD_COPY =
  "Needs internet. Your speech is sent to OpenRouter, using your own account.";
export const KEY_SAVED = "✓ Connected to OpenRouter";
export const KEY_SAVED_COPY = "The key is in your macOS Keychain. It's never shown in full.";
export const LOCAL_ONLY = "I'll use Vocet on this Mac only";

/** The step's title. */
export const SPEECH_TITLE = "How should Vocet transcribe?";

/** How long to wait for a cancelled download to let go, before saying so. */
const CANCEL_TRIES = 15;
const CANCEL_WAIT_MS = 200;

interface Props {
  parts: FrameParts;
  /** The engine selected earlier on this step, while the window is open. */
  kept: SpeechChoice | null;
  onPick: (choice: SpeechChoice) => void;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  models: LocalModelInfo_Serialize[] | null;
  /** The cleanup model (Gemma): the second part of "On this Mac"'s one download. */
  cleanup: LocalModelInfo_Serialize | null;
  modelsError: string | null;
  refreshModels: () => Promise<void>;
  refreshEngine: () => Promise<void>;
  onContinue: () => void;
}

/** The engine picked on the step. */
export type SpeechChoice = "local" | "key";
type Choice = SpeechChoice;

/** Waits while nothing is ready, so the step's own hooks start with settings. */
export function SpeechStep(props: Props) {
  if (props.settings === null) {
    return (
      <StepFrame
        parts={props.parts}
        title={SPEECH_TITLE}
        primary={{ label: "Continue", onClick: () => undefined, blockedBecause: "Loading…" }}
      >
        <p>Loading…</p>
      </StepFrame>
    );
  }
  return <SpeechForm {...props} />;
}

function SpeechForm({
  parts,
  kept,
  onPick,
  onStored,
  models,
  cleanup,
  modelsError,
  refreshModels,
  refreshEngine,
  onContinue,
}: Props) {
  // What the user picked here. Until they do: the recommendation, or the key
  // they already chose (coming back to the step).
  const [picked, setPicked] = useState<Choice | null>(kept);

  function pick(choice: Choice) {
    setPicked(choice);
    setProblem(null);
    onPick(choice);
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

  // Setup changes the engine only on Continue, once OpenRouter is connected.
  const connection = useConnection({ refreshEngine });

  const [keyScreen, setKeyScreen] = useState(false);

  // The Continue running: anything
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

  const model = models === null ? null : recommendedModel(models);
  // OpenRouter is the default (owner decision, Q3); the on-device model is the other.
  const choice: Choice = picked ?? "key";

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

  /** The cleanup model as the app says now; the last poll if it can't be read. */
  async function currentCleanup(): Promise<LocalModelInfo_Serialize | null> {
    try {
      const result = await commands.listCleanupModels();
      if (result.status === "ok") return result.data[0] ?? null;
    } catch {
      // The last poll stands.
    }
    return cleanup;
  }

  async function continueLocal(chosen: LocalModelInfo_Serialize) {
    const picked = await commands.chooseLocalModel(chosen.id);
    if (picked.status === "error") return messageOf(picked.error);
    await makeRoomFor(chosen.id);
    // The download is speech, then cleanup: resume it while either is missing
    // (a restart during the cleanup part leaves the speech model ready). A
    // running cleanup download needs nothing started, but a missing speech
    // model always starts one.
    const gemma = await currentCleanup();
    const speechMissing = chosen.state !== "ready";
    const cleanupMissing =
      gemma !== null && gemma.state !== "ready" && gemma.state !== "downloading";
    if (speechMissing || cleanupMissing) {
      const failed = await startDownload(chosen.id);
      if (failed !== null) return failed;
    }
    await reread();
    await refreshModels();
    await refreshEngine();
    return null;
  }

  async function continueWithKey(dropped: () => boolean) {
    const used = await commands.useCloudEngine("openrouter");
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
  const needsDownload =
    choice === "local" &&
    model !== null &&
    (model.state !== "ready" || (cleanup !== null && cleanup.state !== "ready"));

  /** Back to the choices. `false` if a key is being saved: stay and show it. */
  async function backFromKey(): Promise<boolean> {
    if (!(await connection.canLeave())) return false;
    dropContinue();
    connection.clearNotice();
    setProblem(null);
    setKeyScreen(false);
    return true;
  }

  if (keyScreen) {
    return (
      <KeyScreen
        parts={parts}
        connection={connection}
        problem={problem}
        working={working}
        onBack={() => {
          void backFromKey();
        }}
        onLocalOnly={() => {
          void backFromKey().then((left) => {
            if (left) pick("local");
          });
        }}
        onContinue={() => void go()}
      />
    );
  }

  return (
    <StepFrame
      parts={parts}
      title={SPEECH_TITLE}
      primary={{
        label: needsDownload ? "Download and continue" : "Continue",
        onClick: () => {
          if (choice === "key") {
            onPick("key");
            setProblem(null);
            setKeyScreen(true);
          } else {
            void go();
          }
        },
        blockedBecause,
      }}
    >
      <label className={`ob-card ob-option${choice === "key" ? " selected" : ""}`}>
        <input
          type="radio"
          name="ob-engine"
          aria-label="Use OpenRouter"
          checked={choice === "key"}
          onChange={() => {
            pick("key");
          }}
          // A click on the preselected card changes nothing, so it fires no
          // change: record the choice anyway (a fresh install's default).
          onClick={() => {
            if (choice === "key") pick("key");
          }}
        />
        <span className="ob-option-text">
          <span className="ob-card-title">Use OpenRouter</span>
          <span className="ob-caption">{KEY_CARD_COPY}</span>
        </span>
      </label>

      <label className={`ob-card ob-option${choice === "local" ? " selected" : ""}`}>
        <input
          type="radio"
          name="ob-engine"
          aria-label="Use this Mac"
          checked={choice === "local"}
          disabled={model === null}
          onChange={() => {
            pick("local");
          }}
        />
        <span className="ob-option-text">
          <span className="ob-eyebrow">On this Mac</span>
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

      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}

/**
 * Connect OpenRouter: its own screen under Speech, so the rail doesn't move
 * (U11). The panel is Settings' own (`connection.tsx`), in a page and without
 * Disconnect. Continue unlocks once a key is saved, and a failure stays here.
 */
function KeyScreen({
  parts,
  connection,
  problem,
  working,
  onBack,
  onLocalOnly,
  onContinue,
}: {
  parts: FrameParts;
  connection: Connection;
  problem: string | null;
  working: boolean;
  onBack: () => void;
  onLocalOnly: () => void;
  onContinue: () => void;
}) {
  const { status, busy } = connection;
  const connected = status.state === "connected";
  const blockedBecause = working
    ? "One moment…"
    : status.state === "checking"
      ? "Checking for a saved key…"
      : connected
        ? null
        : KEY_REASON;
  return (
    <StepFrame
      parts={{
        ...parts,
        back: onBack,
        // Once the key is being written nothing leaves, as in Settings' sheet.
        exitBlock: parts.exitBlock ?? (busy === "saving" ? "Saving your key…" : null),
      }}
      title="Connect OpenRouter"
      primary={{ label: "Continue", onClick: onContinue, blockedBecause }}
    >
      {connected && !connection.justConnected && (
        <div className="ob-key-saved">
          <div className="ob-card-text">
            <span role="status" className="ob-key-saved-title">
              {KEY_SAVED}
            </span>
            <span className="ob-caption">{KEY_SAVED_COPY}</span>
          </div>
        </div>
      )}
      <div className="ob-key-panel">
        <ConnectPanel connection={connection} canDisconnect={false} />
      </div>
      <button type="button" className="link" onClick={onLocalOnly}>
        {LOCAL_ONLY}
      </button>
      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}
