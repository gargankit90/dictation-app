import type { Settings_Serialize } from "../bindings";
import { keycapLabel } from "../hotkeyKeys";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";

interface Props {
  parts: FrameParts;
  settings: Settings_Serialize | null;
  onContinue: () => void;
}

/** Welcome: what the app does. The sharing switch is on Ready (U10). */
export function WelcomeStep({ parts, settings, onContinue }: Props) {
  const key = keycapLabel(settings?.hotkey ?? "RightAlt");

  return (
    <StepFrame
      parts={parts}
      title="Talk instead of type"
      primary={{
        label: "Continue",
        onClick: onContinue,
        blockedBecause: settings === null ? "Loading…" : null,
      }}
    >
      <div className="ob-demo">
        <kbd className="keycap">{key}</kbd>
        <span>Hold {key}, speak, release, and the text lands at your cursor.</span>
      </div>
    </StepFrame>
  );
}
