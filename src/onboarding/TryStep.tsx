import { useEffect, useRef, useState } from "react";
import { commands } from "../bindings";
import type { DictationProgressView, OnboardingStatus, Settings_Serialize } from "../bindings";
import { HotkeyRecorder } from "../HotkeyRecorder";
import { messageOf } from "../keyTest";
import { saveHotkey, setHotkeyRecording } from "../saveHotkey";
import { holdsWords, tryView } from "./logic";
import type { PasteCheck } from "./logic";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

/** How often the step asks the app what the last dictation got through. */
export const PROGRESS_POLL_MS = 400;
/** How long a finished paste gets to show up in the box. */
export const PASTE_GRACE_MS = 800;
/** How often the paused hotkey's lease is renewed (the app's lease is 30 s). */
export const HOLD_RENEW_MS = 10_000;

export const CONTINUE_REASON = "Continue unlocks after a dictation lands in the box";

interface Props {
  parts: FrameParts;
  status: OnboardingStatus;
  settings: Settings_Serialize | null;
  onStored: (settings: Settings_Serialize) => void;
  /** Why practice has to wait, or `null` if it can run. */
  wait: string | null;
  onContinue: () => void;
  onSkip: () => void;
  onBackToSpeech: () => void;
}

function Stage({ done, children }: { done: boolean; children: string }) {
  return (
    <li className={done ? "ob-stage done" : "ob-stage"}>
      <span aria-hidden="true">{done ? "✓" : "·"}</span> {children}
      <span className="ob-sr">{done ? " (done)" : " (not yet)"}</span>
    </li>
  );
}

/**
 * Try it: the key, a practice box, and a stage line driven only by what the
 * app reports. "Pasted" also needs the words the app says it pasted to be in
 * the box, so typing there never advances anything. A paste that misses shows
 * the words with Copy and is never retried by itself.
 */
export function TryStep({
  parts,
  status,
  settings,
  onStored,
  wait,
  onContinue,
  onSkip,
  onBackToSpeech,
}: Props) {
  const [box, setBox] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const fixArea = useRef<HTMLDivElement>(null);
  const [progress, setProgress] = useState<DictationProgressView | null>(null);
  const [baseline, setBaseline] = useState<DictationProgressView | null>(null);
  // The paste check, and which dictation it belongs to.
  const [paste, setPaste] = useState<{ started: number; check: PasteCheck } | null>(null);
  const [words, setWords] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [succeeded, setSucceeded] = useState(status.try_outcome === "passed");
  const blocked = wait !== null;

  // The paste goes wherever the focus is: put it in the box.
  useEffect(() => {
    if (!blocked) field.current?.focus();
  }, [blocked]);

  // While practice waits for the model the key does nothing here. The hotkey
  // recorder's pause, renewed; it lapses by itself if this window stops.
  useEffect(() => {
    if (!blocked) return;
    setHotkeyRecording(true);
    const timer = window.setInterval(() => {
      setHotkeyRecording(true);
    }, HOLD_RENEW_MS);
    return () => {
      window.clearInterval(timer);
      setHotkeyRecording(false);
    };
  }, [blocked]);

  useEffect(() => {
    const state = { cancelled: false };
    const poll = async () => {
      try {
        const result = await commands.dictationProgress();
        if (state.cancelled || result.status !== "ok") return;
        setProgress(result.data);
        setBaseline((first) => first ?? result.data);
      } catch {
        // The next poll tries again; nothing shows success meanwhile.
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), PROGRESS_POLL_MS);
    return () => {
      state.cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  const startedNow = progress?.started ?? 0;
  const counted = progress !== null && baseline !== null && progress.started > baseline.started;
  const finishedThis = counted && progress.finished === progress.started;
  const pastedByApp = finishedThis && progress.stages.includes("pasted");
  const pasteFailed = counted && progress.failure === "paste";
  const check = paste !== null && paste.started === startedNow ? paste.check : "unknown";

  // Once the app says it pasted: did the words reach the box? Not right away,
  // since the paste's input event can trail the phase by a moment.
  useEffect(() => {
    if (!pastedByApp && !pasteFailed) return;
    const state = { cancelled: false };
    const look = async (final: boolean) => {
      let text: string | null = null;
      try {
        const result = await commands.practiceWords();
        if (result.status === "ok") text = result.data;
      } catch {
        // Treated as not known: the box can't be compared.
      }
      if (state.cancelled) return;
      setWords(text);
      if (pasteFailed) {
        setPaste({ started: startedNow, check: "missed" });
        return;
      }
      if (text !== null && holdsWords(field.current?.value ?? "", text)) {
        setPaste({ started: startedNow, check: "landed" });
        setSucceeded(true);
        await commands.setTryOutcome("passed").catch(() => undefined);
      } else if (final) {
        setPaste({ started: startedNow, check: "missed" });
      }
    };
    void look(false);
    const timer = window.setTimeout(() => void look(true), PASTE_GRACE_MS);
    return () => {
      state.cancelled = true;
      window.clearTimeout(timer);
    };
  }, [pastedByApp, pasteFailed, startedNow]);

  const view = tryView(progress, baseline, check);

  // role="alert" announces it but doesn't scroll: the panel lands below the
  // fold in a short window. Again when the words arrive and it grows.
  useEffect(() => {
    if (view.fix.kind !== "none") fixArea.current?.scrollIntoView({ block: "nearest" });
  }, [view.fix.kind, words, copied]);

  async function copy() {
    try {
      const result = await commands.copyPracticeWords();
      setCopied(result.status === "ok" ? "Copied." : messageOf(result.error));
    } catch {
      setCopied("Couldn't copy the words. Select them and press ⌘C.");
    }
  }

  return (
    <StepFrame
      parts={parts}
      title="Try it"
      primary={{
        label: "Continue",
        onClick: onContinue,
        blockedBecause: succeeded ? null : CONTINUE_REASON,
      }}
      secondary={succeeded ? undefined : { label: "Skip for now", onClick: onSkip }}
    >
      <span className="ob-label">Your dictation key</span>
      <HotkeyRecorder
        value={settings?.hotkey ?? "RightAlt"}
        disabled={settings === null}
        onRecording={(recording) => {
          // While practice waits, the key stays paused after recording too.
          setHotkeyRecording(recording || blocked);
        }}
        onChange={async (spec) => {
          const result = await saveHotkey(spec);
          if (result.settings !== null) onStored(result.settings);
          return result.error;
        }}
      />
      <label htmlFor="try-field" className="ob-label">
        Click the box, hold the key, say a sentence, let go.
      </label>
      <textarea
        id="try-field"
        className="ob-practice"
        ref={field}
        rows={3}
        value={box}
        disabled={blocked}
        placeholder={wait ?? undefined}
        onChange={(event) => {
          setBox(event.target.value);
        }}
      />
      {wait !== null && (
        <p role="status" className="ob-sr">
          {wait}
        </p>
      )}
      <ol className="ob-stages" aria-label="What Vocet did">
        <Stage done={view.heard}>Heard you</Stage>
        <Stage done={view.transcribed}>Transcribed</Stage>
        <Stage done={view.pasted}>Pasted</Stage>
      </ol>

      {view.pasted && (
        <p role="status" className="notice ok">
          It works. The same key works in any app.
        </p>
      )}
      {view.fix.kind !== "none" && (
        <div className="ob-fix-area" ref={fixArea}>
          {view.fix.kind === "microphone" && (
            <div role="alert" className="ob-fix">
              <p>{view.fix.message}</p>
              <p className="ob-caption">
                Check the input in System Settings → Sound, and that it isn&apos;t muted.
              </p>
            </div>
          )}
          {view.fix.kind === "key" && (
            <div role="alert" className="ob-fix">
              <p>{view.fix.message}</p>
              <div className="row">
                <button type="button" onClick={onBackToSpeech}>
                  Back to Speech
                </button>
              </div>
            </div>
          )}
          {view.fix.kind === "other" && (
            <p role="alert" className="ob-fix">
              {view.fix.message}
            </p>
          )}
          {view.fix.kind === "paste" && (
            <div role="alert" className="ob-fix">
              <p>Vocet heard you, but the words didn&apos;t reach the box:</p>
              {words !== null && <p className="ob-words">{words}</p>}
              <div className="row">
                <button type="button" disabled={words === null} onClick={() => void copy()}>
                  Copy
                </button>
              </div>
              <p className="ob-caption">
                Click the box and press ⌘V. Vocet never pastes again by itself, so nothing lands
                twice.
              </p>
              {copied !== null && (
                <p role="status" className="ob-caption">
                  {copied}
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </StepFrame>
  );
}
