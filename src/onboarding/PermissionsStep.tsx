import { useState } from "react";
import type { ReactNode } from "react";
import { commands } from "../bindings";
import type { AppPermission, OnboardingStatus, PermissionState } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { MicMeter, useLiveMeter } from "../settings/MicMeter";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const BOTH_REASON = "Continue unlocks when both are allowed";
const MIC_PATH = "System Settings → Privacy & Security → Microphone → turn on Vocet.";
const ACCESSIBILITY_PATH = "System Settings → Privacy & Security → Accessibility → turn on Vocet.";

interface Props {
  parts: FrameParts;
  status: OnboardingStatus;
  onStatus: (status: OnboardingStatus) => void;
  /** Accessibility has looked allowed-but-not-applied for a while. */
  needsRelaunch: boolean;
  onContinue: () => void;
}

function Relaunch({
  busy,
  onRelaunch,
  children,
}: {
  busy: boolean;
  onRelaunch: () => void;
  children: ReactNode;
}) {
  return (
    <>
      <p className="ob-caption">{children}</p>
      <div className="row">
        <button type="button" disabled={busy} onClick={onRelaunch}>
          Relaunch and resume
        </button>
      </div>
    </>
  );
}

/**
 * Permissions: each card says what the permission is for before macOS asks,
 * then follows what the OS reports (the shell polls it). A grant the running
 * app can't pick up gets "Relaunch and resume"; setup comes back at this step
 * with the choices kept.
 */
export function PermissionsStep({ parts, status, onStatus, needsRelaunch, onContinue }: Props) {
  const [asked, setAsked] = useState<Partial<Record<AppPermission, boolean>>>({});
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const micAllowed = status.microphone === "granted";
  const meter = useLiveMeter(micAllowed);

  async function ask(which: AppPermission) {
    setBusy(true);
    setProblem(null);
    try {
      const result = await commands.requestPermission(which);
      if (result.status === "ok") {
        onStatus(result.data);
        setAsked((now) => ({ ...now, [which]: true }));
      } else {
        setProblem(messageOf(result.error));
      }
    } catch {
      setProblem(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  async function relaunch() {
    setBusy(true);
    setProblem(null);
    try {
      const result = await commands.relaunchApp();
      if (result.status === "error") setProblem(messageOf(result.error));
    } catch {
      setProblem(BROKEN);
    } finally {
      setBusy(false);
    }
  }

  const allowed = micAllowed && status.accessibility === "granted";

  function badge(state: PermissionState, which: AppPermission) {
    if (state === "granted") return <span className="ob-allowed">✓ Allowed</span>;
    if (state === "denied") return <span className="ob-attention">Turned off</span>;
    if (asked[which] === true) return <span className="ob-waiting">Waiting for macOS…</span>;
    return (
      <button
        type="button"
        className="ob-allow"
        aria-label={`Allow ${which === "microphone" ? "Microphone" : "Accessibility"}`}
        disabled={busy}
        onClick={() => void ask(which)}
      >
        Allow
      </button>
    );
  }

  return (
    <StepFrame
      parts={parts}
      title="Two permissions"
      primary={{
        label: "Continue",
        onClick: onContinue,
        blockedBecause: allowed ? null : BOTH_REASON,
      }}
    >
      <p>macOS asks you to confirm each one. This screen updates on its own.</p>

      <section className="ob-card" aria-label="Microphone">
        <div className="ob-card-head">
          <span className="ob-card-text">
            <span className="ob-card-title">Microphone</span>
            <span className="ob-caption">Records only while you hold your key.</span>
          </span>
          {badge(status.microphone, "microphone")}
        </div>
        {micAllowed && (
          <div className="ob-meter">
            <div className="mic-test">
              <MicMeter levels={meter.levels} />
              <span>
                {meter.device ?? "The Mac's default input"}: say something, the bars should move.
              </span>
            </div>
            {meter.error !== null && (
              <p role="alert" className="error">
                {meter.error}
              </p>
            )}
            <p className="ob-caption">No sound? Check System Settings → Sound.</p>
          </div>
        )}
        {status.microphone === "denied" && (
          <>
            <p className="ob-caption">{MIC_PATH}</p>
            <div className="row">
              <button type="button" disabled={busy} onClick={() => void ask("microphone")}>
                Open System Settings
              </button>
            </div>
            <Relaunch busy={busy} onRelaunch={() => void relaunch()}>
              Already turned it on? macOS applies it to Vocet after a relaunch. Your choices are
              kept.
            </Relaunch>
          </>
        )}
      </section>

      <section className="ob-card" aria-label="Accessibility">
        <div className="ob-card-head">
          <span className="ob-card-text">
            <span className="ob-card-title">Accessibility</span>
            <span className="ob-caption">
              Notices your key and pastes at your cursor. It doesn&apos;t read your screen.
            </span>
          </span>
          {badge(status.accessibility, "accessibility")}
        </div>
        {status.accessibility !== "granted" && asked.accessibility === true && (
          <>
            <p className="ob-caption">{ACCESSIBILITY_PATH}</p>
            <div className="row">
              <button type="button" disabled={busy} onClick={() => void ask("accessibility")}>
                Open System Settings
              </button>
            </div>
            <Relaunch busy={busy} onRelaunch={() => void relaunch()}>
              Turned it on and nothing changed? A relaunch makes it take. Your choices are kept.
            </Relaunch>
          </>
        )}
        {needsRelaunch && (
          <Relaunch busy={busy} onRelaunch={() => void relaunch()}>
            macOS says it&apos;s allowed, but Vocet can&apos;t use it until it restarts. Your
            choices are kept.
          </Relaunch>
        )}
      </section>

      {problem !== null && (
        <p role="alert" className="notice error">
          {problem}
        </p>
      )}
    </StepFrame>
  );
}
