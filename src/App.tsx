import { useEffect, useState } from "react";
import { BROKEN, messageOf } from "./keyTest";
import { commands } from "./bindings";
import type { View } from "./bindings";
import { Onboarding } from "./Onboarding";
import { Settings } from "./Settings";
import type { SectionId } from "./Settings";
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
  // The tray's "Report a problem…": Troubleshoot with the report built.
  const [autoReport, setAutoReport] = useState(false);
  // The tray's fix rows: Settings open at the section that fixes it
  // (Speech, or Dictation for the shortcut). `id` makes a repeated ask count.
  const [open, setOpen] = useState<{ section: SectionId; id: number } | null>(null);

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
      let request = "";
      if (next === "settings") {
        try {
          const asked = await commands.takeWindowRequest();
          if (asked.status === "ok") request = asked.data;
        } catch {
          // Settings.
        }
      }
      if (state.cancelled) return;
      setView(next);
      if (request === "troubleshoot" || request === "report") {
        setAutoReport(request === "report");
        setPage("troubleshoot");
      } else if (request === "speech" || request === "shortcut") {
        setPage("settings");
        const section: SectionId = request === "speech" ? "speech" : "dictation";
        setOpen((current) => ({ section, id: (current?.id ?? 0) + 1 }));
      }
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
        autoReport={autoReport}
        onBack={() => {
          setAutoReport(false);
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
      open={open}
      onSetup={() => {
        void (async () => {
          try {
            await commands.openSetup();
            const status = await commands.getOnboarding();
            if (status.status === "ok" && status.data.view === "onboarding") setView("onboarding");
          } catch {
            // Stay on Settings.
          }
        })();
      }}
      onTroubleshoot={() => {
        setPage("troubleshoot");
      }}
      onReport={() => {
        setAutoReport(true);
        setPage("troubleshoot");
      }}
      onShowLog={async () => {
        // The same command as Troubleshoot's, through the app's Opener.
        try {
          const result = await commands.revealLogFile();
          return result.status === "error" ? messageOf(result.error) : null;
        } catch {
          return BROKEN;
        }
      }}
    />
  );
}
