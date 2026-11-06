import { useCallback, useEffect, useRef, useState } from "react";
import { BROKEN, messageOf } from "./keyTest";
import { commands } from "./bindings";
import type { View, WindowPage } from "./bindings";
import { Onboarding } from "./Onboarding";
import { Report } from "./Report";
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
  // Settings, Troubleshoot (E1) or Report a problem (U12).
  const [page, setPage] = useState<WindowPage>("settings");
  // Where Report's Back goes.
  const [reportFrom, setReportFrom] = useState<"settings" | "troubleshoot">("settings");
  // The tray's fix rows: Settings open at the section that fixes it (Speech,
  // or Dictation for the dictation key), its "History…" (U7) and "Help…"
  // (U12). `id` makes a repeated ask count; Settings' `onOpened` clears the
  // ask, so a remount doesn't follow it again (#277).
  const [open, setOpen] = useState<{ section: SectionId; id: number } | null>(null);
  const lastAsk = useRef(0);
  const openSection = useCallback((section: SectionId) => {
    lastAsk.current += 1;
    setPage("settings");
    setOpen({ section, id: lastAsk.current });
  }, []);
  const followed = useCallback(() => {
    setOpen(null);
  }, []);

  // Each screen has its own window title (U12). The webview can't set it
  // itself: it has no `core:` permissions.
  useEffect(() => {
    if (view !== "settings") return;
    void commands.setWindowPage(page).catch(() => undefined);
  }, [view, page]);

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
      if (request === "troubleshoot") {
        setPage("troubleshoot");
      } else if (request === "settings") {
        setPage("settings");
      } else if (request === "shortcut") {
        openSection("dictation");
      } else if (request === "speech" || request === "history" || request === "help") {
        openSection(request);
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
  }, [openSection]);

  if (view === null) return null;
  if (view === "onboarding") {
    return (
      <Onboarding
        onFinished={() => {
          setView("settings");
        }}
        onOpenHistory={() => {
          setView("settings");
          openSection("history");
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
        onReport={() => {
          setReportFrom("troubleshoot");
          setPage("report");
        }}
      />
    );
  }
  if (page === "report") {
    return (
      <Report
        backLabel={reportFrom === "troubleshoot" ? "Back to Troubleshoot" : "Back to Settings"}
        onBack={() => {
          setPage(reportFrom);
        }}
      />
    );
  }
  return (
    <Settings
      open={open}
      onOpened={followed}
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
        setReportFrom("settings");
        setPage("report");
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
