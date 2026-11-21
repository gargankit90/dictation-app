import { useCallback, useEffect, useState } from "react";
import { commands } from "./bindings";
import type { OnboardingStatus, Settings_Serialize, Step } from "./bindings";
import type { SpeechChoice } from "./onboarding/SpeechStep";
import { BROKEN, messageOf } from "./messages";
import { chosenModel, modelWait, stripView } from "./onboarding/logic";
import type { StripAction } from "./onboarding/logic";
import { PermissionsStep } from "./onboarding/PermissionsStep";
import { SpeechStep } from "./onboarding/SpeechStep";
import { TryStep } from "./onboarding/TryStep";
import { WelcomeStep } from "./onboarding/WelcomeStep";
import type { FrameParts } from "./onboarding/StepFrame";
import { useEngineStatus, useModels } from "./Settings";
import { useCleanupModels, withCleanup } from "./settings/CleanupModel";
import "./settings.css";

export const STEPS: readonly { id: Step; name: string }[] = [
  { id: "welcome", name: "Welcome" },
  { id: "speech", name: "Speech" },
  { id: "permissions", name: "Permissions" },
  { id: "try", name: "Try it" },
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
}

/**
 * The first-run steps (U8, TOS1): Welcome, Speech, Permissions, Try it. The
 * app keeps the step and refuses to skip one that isn't done. This shell shows
 * the rail, the persistent download strip and whichever step the app says, and
 * each step decides its own primary action; Done on Try it ends setup.
 */
export function Onboarding({ onFinished }: Props) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings_Serialize | null>(null);
  const { models, error: modelsError, refresh: refreshModels } = useModels();
  const { engine, refresh: refreshEngine } = useEngineStatus();
  const cleanupModels = useCleanupModels(
    settings?.transcription === "local" || status?.step === "speech",
  );
  const [downPolls, setDownPolls] = useState(0);
  // The engine picked on Speech, kept while the window is open.
  const [speechChoice, setSpeechChoice] = useState<SpeechChoice | null>(null);

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

  const download = useCallback(
    async (id: string) => {
      try {
        const result = await commands.downloadLocalModel(id);
        if (result.status === "error") setError(messageOf(result.error));
      } catch {
        setError(BROKEN);
      }
      await refreshModels();
    },
    [refreshModels],
  );

  async function stripAction(action: StripAction, id: string) {
    setError(null);
    if (action === "cancel") {
      try {
        await commands.cancelLocalModelDownload(id);
      } catch {
        setError(BROKEN);
      }
      await refreshModels();
      await refresh();
    } else if (action === "use-key") {
      await moveTo("speech");
    } else {
      await download(id);
      await refresh();
    }
  }

  /**
   * Done: applies "Open Vocet when I log in" (Try it's checkbox; `null` when
   * setup ends from Permissions and has none), then ends setup.
   */
  async function finishNow(openAtLogin: boolean | null = null) {
    setBusy(true);
    try {
      if (openAtLogin !== null) {
        const login = await commands.setLaunchAtLogin(openAtLogin);
        if (login.status === "error") {
          setError(messageOf(login.error));
          return;
        }
      }
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
  // "On this Mac" is one download: speech, then cleanup (Q4).
  const onDevice = models?.map((model) => withCleanup(model, cleanupModels?.[0])) ?? null;
  const strip = stripView(onDevice, chosenModel(settings, onDevice));
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
            void moveTo(previous.id);
          },
    busy,
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
    if (next !== undefined) void moveTo(next.id);
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
        <WelcomeStep parts={parts} settings={settings} onContinue={forward} />
      )}
      {status.step === "speech" && (
        <SpeechStep
          parts={parts}
          kept={speechChoice}
          onPick={setSpeechChoice}
          settings={settings}
          onStored={setSettings}
          models={models}
          cleanup={cleanupModels?.[0] ?? null}
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
          onDone={() => void finishNow()}
        />
      )}
      {status.step === "try" && (
        <TryStep
          parts={parts}
          settings={settings}
          onStored={setSettings}
          wait={modelWait(settings, models, engine)}
          onDone={(openAtLogin) => void finishNow(openAtLogin)}
        />
      )}
    </main>
  );
}
