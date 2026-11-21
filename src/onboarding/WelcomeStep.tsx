import { useState } from "react";
import { commands } from "../bindings";
import type { Link, Settings_Serialize } from "../bindings";
import { DEFAULT_HOTKEY, keyLongLabel, keySpokenName } from "../hotkeyKeys";
import { BROKEN, messageOf } from "../messages";
import { StepFrame } from "./StepFrame";
import type { FrameParts } from "./StepFrame";
import { WelcomeDemo } from "./WelcomeDemo";

interface Props {
  parts: FrameParts;
  settings: Settings_Serialize | null;
  onContinue: () => void;
}

function open(link: Link) {
  void commands.openLink(link).catch(() => undefined);
}

/**
 * Welcome: shows what Vocet does and asks for one thing, the Terms (TOS1).
 * Get started is the agreement: the app keeps the Terms' date and usage
 * reports may start. Nothing is sent before it.
 */
export function WelcomeStep({ parts, settings, onContinue }: Props) {
  const key = settings?.hotkey ?? DEFAULT_HOTKEY;
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  async function getStarted() {
    setWorking(true);
    setProblem(null);
    let failed: string | null = null;
    try {
      const result = await commands.acceptTerms();
      if (result.status === "error") failed = messageOf(result.error);
    } catch {
      failed = BROKEN;
    }
    setWorking(false);
    if (failed === null) onContinue();
    else setProblem(failed);
  }

  return (
    <StepFrame
      parts={{ ...parts, error: parts.error ?? problem }}
      title="Talk instead of type"
      large
      below={
        <p className="ob-terms">
          By continuing, you agree to the{" "}
          <button
            type="button"
            className="link"
            onClick={() => {
              open("terms");
            }}
          >
            Terms
          </button>{" "}
          and{" "}
          <button
            type="button"
            className="link"
            onClick={() => {
              open("privacy");
            }}
          >
            Privacy Policy
          </button>
          .
        </p>
      }
      primary={{
        label: "Get started",
        onClick: () => void getStarted(),
        blockedBecause: settings === null ? "Loading…" : working ? "One moment…" : null,
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
