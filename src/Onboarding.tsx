import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "./bindings";
import type { OnboardingStatus, Settings_Serialize, Step } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";
import { chosenModel, modelWait, stripView } from "./onboarding/logic";
import type { StripAction } from "./onboarding/logic";
import { PermissionsStep } from "./onboarding/PermissionsStep";
import { ReadyStep } from "./onboarding/ReadyStep";
import { SpeechStep } from "./onboarding/SpeechStep";
import { TryStep } from "./onboarding/TryStep";
import { WelcomeStep } from "./onboarding/WelcomeStep";
import type { FrameParts } from "./onboarding/StepFrame";
import { FINISH_REASON } from "./onboarding/ReadyStep";
import { SAVE_FAILED_REASON } from "./onboarding/WelcomeStep";
import { sharingBlock, useSharingSwitch } from "./settings/Privacy";
import { useEngineStatus, useModels } from "./Settings";
import "./settings.css";

export const STEPS: readonly { id: Step; name: string }[] = [
  { id: "welcome", name: "Welcome" },
  { id: "speech", name: "Speech" },
  { id: "permissions", name: "Permissions" },
  { id: "try", name: "Try it" },
  { id: "ready", name: "Ready" },
];

/** How often the shell asks the app about permissions, the model and the engine. */
export const POLL_MS = 1000;
/** Polls in a row the hotkey may be down after a grant before offering a relaunch. */
export const RELAUNCH_AFTER_POLLS = 5;

const ACTION_LABELS: Record<StripAction, string> = {
  cancel: "Cancel",
  resume: "Resume",
  retry: "Retry",
  "use-key": "Use my own key",
};

interface Props {
  /** Called once the user is done and the app has hidden the window. */
  onFinished: () => void;
  /** Called once the app has left the steps for Settings → History. */
  onOpenHistory?: () => void;
}

/**
 * The first-run steps (U8): Welcome, Speech, Permissions, Try it, Ready. The
 * app keeps the step and refuses to skip one that isn't done; it remembers
 * the step across quits. This shell shows the rail, the persistent download
 * strip and whichever step the app says, and each step decides its own
 * primary action.
 */
export function Onboarding({ onFinished, onOpenHistory }: Props) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings_Serialize | null>(null);
  const { models, error: modelsError, refresh: refreshModels } = useModels();
  const { engine, refresh: refreshEngine } = useEngineStatus();
  const [downPolls, setDownPolls] = useState(0);
  const resumeChecked = useRef(false);

  // Here, not in a step: the writes outlive any one step, and every way out
  // of a step waits for them, since leaving can send an event.
  const sharing = useSharingSwitch(settings?.share_usage_events ?? true, setSettings);
  const exitBlock = sharingBlock(
    sharing.save,
    status?.step === "ready" ? FINISH_REASON : SAVE_FAILED_REASON,
  );
  /**
   * The one way out of a step: moving, finishing, or leaving for Settings all
   * go through here, so a new exit can't overtake the user's sharing choice.
   */
  const leave = useCallback(
    async (exit: () => Promise<void>) => {
      if (exitBlock === null) await exit();
    },
    [exitBlock],
  );

  const moveTo = useCallback(async (step: Step) => {
    setBusy(true);
    try {
      const result = await commands.setOnboardingStep(step);
      if (result.status === "ok") {
        setStatus(result.data);
        setError(null);
      } else {
        setError(messageOf(result.error));
      }
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }, []);

  const goTo = useCallback((step: Step) => leave(() => moveTo(step)), [leave, moveTo]);

  const refresh = useCallback(async () => {
    try {
      const result = await commands.getOnboarding();
      if (result.status === "error") {
        setLoadError(messageOf(result.error));
        return;
      }
      setStatus(result.data);
      setDownPolls((count) => (result.data.needs_relaunch ? count + 1 : 0));
      setLoadError(null);
    } catch {
      setLoadError(BROKEN);
    }
  }, []);

  const rereadSettings = useCallback(async () => {
    try {
      const result = await commands.getSettings();
      if (result.status === "ok") setSettings(result.data);
      else setError(messageOf(result.error));
    } catch {
      setError(BROKEN);
    }
  }, []);

  // Load, and ask again whenever the window comes back or a second passes:
  // the answers come from system prompts and System Settings, and from a
  // download that goes on by itself.
  useEffect(() => {
    const first = setTimeout(() => {
      void refresh();
      void rereadSettings();
    }, 0);
    const onShow = () => {
      if (document.visibilityState !== "hidden") {
        void refresh();
        void rereadSettings();
      }
    };
    const timer = window.setInterval(() => {
      if (document.visibilityState === "hidden") return;
      void refresh();
      void refreshEngine();
      void refreshModels();
    }, POLL_MS);
    window.addEventListener("focus", onShow);
    document.addEventListener("visibilitychange", onShow);
    return () => {
      clearTimeout(first);
      window.clearInterval(timer);
      window.removeEventListener("focus", onShow);
      document.removeEventListener("visibilitychange", onShow);
    };
  }, [refresh, rereadSettings, refreshEngine, refreshModels]);

  const chosen = chosenModel(settings, models);

  const download = useCallback(
    async (id: string) => {
      try {
        await commands.setDownloadPaused(false);
        const result = await commands.downloadLocalModel(id);
        if (result.status === "error") setError(messageOf(result.error));
      } catch {
        setError(BROKEN);
      }
      await refreshModels();
    },
    [refreshModels],
  );

  // A download that a quit interrupted picks up by itself, unless the user
  // cancelled it. Once per launch, once everything it depends on is known.
  const step = status?.step ?? null;
  const paused = status?.download_paused ?? false;
  useEffect(() => {
    if (resumeChecked.current || models === null || settings === null || step === null) return;
    resumeChecked.current = true;
    if (chosen === null || paused || step === "welcome" || step === "speech") return;
    if (chosen.state !== "partial" && chosen.state !== "missing") return;
    if (chosen.error !== null) return;
    // After this effect, and not on a timer its cleanup could cancel:
    // resumeChecked is already set, so nothing would start it again.
    const id = chosen.id;
    queueMicrotask(() => void download(id));
  }, [models, settings, step, chosen, paused, download]);

  async function stripAction(action: StripAction, id: string) {
    setError(null);
    if (action === "cancel") {
      try {
        await commands.setDownloadPaused(true);
        await commands.cancelLocalModelDownload(id);
      } catch {
        setError(BROKEN);
      }
      await refreshModels();
      await refresh();
    } else if (action === "use-key") {
      await goTo("speech");
    } else {
      await download(id);
      await refresh();
    }
  }

  async function finishNow() {
    setBusy(true);
    try {
      const result = await commands.finishOnboarding();
      if (result.status === "error") {
        setError(messageOf(result.error));
        return;
      }
      onFinished();
    } catch {
      setError(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  if (status === null) {
    return (
      <main className="settings onboarding">
        <h1>Set up Vocet</h1>
        {loadError === null ? (
          <p>Loading…</p>
        ) : (
          <p role="alert" className="notice error">
            Couldn&apos;t load the setup: {loadError}
          </p>
        )}
      </main>
    );
  }

  const index = STEPS.findIndex((item) => item.id === status.step);
  const previous = STEPS[index - 1];
  const next = STEPS[index + 1];
  const strip = stripView(models, chosen);
  // A strip only for a running download on the first two steps: until the
  // user has chosen, a stopped one isn't news.
  const stripShown =
    strip !== null &&
    (strip.actions.includes("cancel") || index >= STEPS.findIndex((s) => s.id === "permissions"));
  const downloading = models?.some((model) => model.state === "downloading") ?? false;

  const parts: FrameParts = {
    back:
      previous === undefined
        ? null
        : () => {
            void goTo(previous.id);
          },
    busy,
    exitBlock,
    error,
    strip: stripShown ? (
      <div className="ob-strip" role="status" aria-label="Model download">
        <div className="ob-strip-row">
          <span className="ob-strip-text">{strip.text}</span>
          {strip.percent !== null && (
            <progress aria-label="Download progress" max={100} value={strip.percent} />
          )}
          {strip.actions.map((action) => (
            <button
              key={action}
              type="button"
              className="link"
              disabled={action === "use-key" && exitBlock !== null}
              onClick={() => void stripAction(action, strip.model.id)}
            >
              {ACTION_LABELS[action]}
            </button>
          ))}
        </div>
        {strip.note !== null && <p className="ob-caption">{strip.note}</p>}
      </div>
    ) : null,
  };
  const forward = () => {
    if (next !== undefined) void goTo(next.id);
  };

  return (
    <main className="onboarding ob">
      <nav className="ob-rail" aria-label="Setup steps">
        <ol>
          {STEPS.map((item, position) => {
            const state = position < index ? "done" : position === index ? "current" : "upcoming";
            return (
              <li
                key={item.id}
                className={`ob-step ${state}`}
                aria-current={state === "current" ? "step" : undefined}
              >
                <span className="ob-mark" aria-hidden="true">
                  {state === "done" ? "✓" : String(position + 1)}
                </span>
                <span className="ob-step-text">
                  <span>{item.name}</span>
                  {item.id === "speech" && downloading && (
                    <span className="ob-step-note">Download running</span>
                  )}
                </span>
              </li>
            );
          })}
        </ol>
      </nav>

      {status.step === "welcome" && (
        <WelcomeStep parts={parts} settings={settings} onContinue={forward} sharing={sharing} />
      )}
      {status.step === "speech" && (
        <SpeechStep
          parts={parts}
          kept={status.speech_choice ?? null}
          settings={settings}
          onStored={setSettings}
          models={models}
          modelsError={modelsError}
          refreshModels={refreshModels}
          refreshEngine={refreshEngine}
          onContinue={forward}
        />
      )}
      {status.step === "permissions" && (
        <PermissionsStep
          parts={parts}
          status={status}
          onStatus={setStatus}
          needsRelaunch={downPolls >= RELAUNCH_AFTER_POLLS}
          onContinue={forward}
        />
      )}
      {status.step === "try" && (
        <TryStep
          parts={parts}
          status={status}
          settings={settings}
          onStored={setSettings}
          wait={modelWait(settings, models, engine)}
          onContinue={forward}
          onSkip={() => {
            void (async () => {
              await commands.setTryOutcome("skipped").catch(() => undefined);
              await goTo("ready");
            })();
          }}
          onBackToSpeech={() => void goTo("speech")}
        />
      )}
      {status.step === "ready" && (
        <ReadyStep
          parts={parts}
          status={status}
          settings={settings}
          onStored={setSettings}
          models={models}
          engine={engine}
          onChangeHistory={() => {
            void leave(async () => {
              try {
                const left = await commands.leaveSetupForSettings();
                if (left.status === "error") setError(messageOf(left.error));
                else onOpenHistory?.();
              } catch {
                setError(BROKEN);
              }
            });
          }}
          sharing={sharing}
          onFinish={() => void leave(finishNow)}
        />
      )}
    </main>
  );
}
