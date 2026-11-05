import type { Settings_Serialize } from "../bindings";
import { DEFAULT_HOTKEY, keyLongLabel, keySpokenName } from "../hotkeyKeys";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";
import { WelcomeDemo } from "./WelcomeDemo";

interface Props {
  parts: FrameParts;
  settings: Settings_Serialize | null;
  onContinue: () => void;
}

/**
 * Welcome: shows what Vocet does and asks for nothing (U11). The sharing switch
 * is on Ready (U10).
 */
export function WelcomeStep({ parts, settings, onContinue }: Props) {
  const key = settings?.hotkey ?? DEFAULT_HOTKEY;
  return (
    <StepFrame
      parts={parts}
      title="Talk instead of type"
      large
      primary={{
        label: "Get started",
        onClick: onContinue,
        blockedBecause: settings === null ? "Loading…" : null,
      }}
    >
      <p className="ob-lede">Vocet turns what you say into text, in any app.</p>
      <p className="ob-sr">
        Hold {keySpokenName(key)}, say what you mean, and let go. The words land at your cursor.
      </p>
      <WelcomeDemo keyLabel={keyLongLabel(key)} />
    </StepFrame>
  );
}
