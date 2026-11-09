import { useState } from "react";
import type { ReactNode } from "react";
import { commands } from "../bindings";
import type { AppPermission, OnboardingStatus, PermissionState } from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { MicMeter, useLiveMeter } from "../settings/MicMeter";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

export const BOTH_REASON = "Continue unlocks when both are allowed";
export const BOTH_REASON_DONE = "Done unlocks when both are allowed";
const MIC_PATH = "System Settings → Privacy & Security → Microphone → turn on Vocet.";
const ACCESSIBILITY_PATH = "System Settings → Privacy & Security → Accessibility → turn on Vocet.";
export const ASKED_AGAIN = "macOS is asking for these again";
/** The Accessibility fix after an update signed differently (Design Lead, #309). */
export const OLD_ROW_LEAD = "If Vocet already looks on there, that row belongs to the old version:";
export const OLD_ROW_STEPS = [
  "In System Settings → Privacy & Security → Accessibility, select Vocet and click −.",
  "Click Allow here, then turn on the new Vocet row.",
  "No new row? Click + and add Vocet from Applications.",
];

interface Props {
  parts: FrameParts;
  status: OnboardingStatus;
  onStatus: (status: OnboardingStatus) => void;
  /** Accessibility has looked allowed-but-not-applied for a while. */
  needsRelaunch: boolean;
  onContinue: () => void;
  /** Ends setup at once, for someone who had already finished it. */
  onDone: () => void;
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
export function PermissionsStep({
  parts,
  status,
  onStatus,
  needsRelaunch,
  onContinue,
  onDone,
}: Props) {
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
  // Setup was finished, and a permission has gone since (N1, #206): most likely an
  // update signed differently. The notice's promise holds once both are allowed:
  // the window closes, with no second setup.
  const askedAgain = status.asked_again;

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
        label: askedAgain ? "Done" : "Continue",
        onClick: askedAgain ? onDone : onContinue,
        blockedBecause: allowed ? null : askedAgain ? BOTH_REASON_DONE : BOTH_REASON,
      }}
    >
      {askedAgain && (
        <section className="notice" aria-label={ASKED_AGAIN}>
          <p>
            <strong>{ASKED_AGAIN}.</strong> This happens after an update that changes how Vocet is
            signed, or if a permission was turned off. Your settings, history and saved key are all
            still here.
          </p>
        </section>
      )}
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
        {askedAgain && status.accessibility !== "granted" && (
          <div className="ob-old-row" aria-label="Replace the old Accessibility row">
            <p className="ob-caption">{OLD_ROW_LEAD}</p>
            <ol className="ob-steps">
              {OLD_ROW_STEPS.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
        )}
        {status.accessibility !== "granted" && asked.accessibility === true && (
          <>
            <p className="ob-caption">{ACCESSIBILITY_PATH}</p>
            <div className="row">
              <button type="button" disabled={busy} onClick={() => void ask("accessibility")}>
                Open System Settings
              </button>
            </div>
            <Relaunch busy={busy} onRelaunch={() => void relaunch()}>
              {askedAgain
                ? "Turned on the new Vocet row and nothing changed? A relaunch makes it take. Turning the old row off and on won't help: remove it, as above. Your choices are kept."
                : "Turned it on and nothing changed? A relaunch makes it take. Your choices are kept."}
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
