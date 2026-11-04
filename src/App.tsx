import { useEffect, useState } from "react";
import { commands } from "./bindings";
import type { View } from "./bindings";
import { Onboarding } from "./Onboarding";
import { Settings } from "./Settings";
import { Troubleshoot } from "./Troubleshoot";

/**
 * The app's one window (opened from the tray): the settings form, or the
 * first-run steps (M5.2). The app decides which; the window asks when it
 * loads and whenever it is shown again.
 */
export function App() {
  // `null` until the app answers.
  const [view, setView] = useState<View | null>(null);
  // Settings, or Troubleshoot (E1), which the tray can ask for.
  const [page, setPage] = useState<"settings" | "troubleshoot">("settings");

  // ⌘, opens Settings (U6), as in every Mac app, from Troubleshoot too.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey && event.key === ",") {
        event.preventDefault();
        setPage("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, []);

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
      let troubleshoot = false;
      if (next === "settings") {
        try {
          const asked = await commands.takeTroubleshootRequest();
          troubleshoot = asked.status === "ok" && asked.data;
        } catch {
          // Settings.
        }
      }
      if (state.cancelled) return;
      setView(next);
      if (troubleshoot) setPage("troubleshoot");
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
  if (page === "troubleshoot") {
    return (
      <Troubleshoot
        onBack={() => {
          setPage("settings");
        }}
        onOpenSettings={() => {
          setPage("settings");
        }}
      />
    );
  }
  return (
    <Settings
      onTroubleshoot={() => {
        setPage("troubleshoot");
      }}
      onReport={() => {
        setPage("troubleshoot");
      }}
    />
  );
}
