import { useEffect, useState } from "react";
import { commands } from "./bindings";
import type { View } from "./bindings";
import { Onboarding } from "./Onboarding";
import { Settings } from "./Settings";

/**
 * The app's one window (opened from the tray): the settings form, or the
 * first-run steps (M5.2). The app decides which; the window asks when it
 * loads and whenever it is shown again.
 */
export function App() {
  // `null` until the app answers.
  const [view, setView] = useState<View | null>(null);

  useEffect(() => {
    const state = { cancelled: false };
    const ask = async () => {
      let next: View = "settings";
      try {
        const result = await commands.getOnboarding();
        if (result.status === "ok" && result.data.view === "onboarding") next = "onboarding";
      } catch {
        // Settings, which works without onboarding.
      }
      if (!state.cancelled) setView(next);
    };
    void ask();
    const onShow = () => {
      if (document.visibilityState !== "hidden") void ask();
    };
    window.addEventListener("focus", onShow);
    document.addEventListener("visibilitychange", onShow);
    return () => {
      state.cancelled = true;
      window.removeEventListener("focus", onShow);
      document.removeEventListener("visibilitychange", onShow);
    };
  }, []);

  if (view === null) return null;
  if (view === "onboarding") {
    return (
      <Onboarding
        onFinished={() => {
          setView("settings");
        }}
      />
    );
  }
  return <Settings />;
}
