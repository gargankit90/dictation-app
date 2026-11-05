import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { commands } from "./bindings";
import type { EngineStatus, LocalModelInfo_Serialize, Settings_Serialize } from "./bindings";
import { BROKEN, messageOf } from "./keyTest";
import { cloudLanguages } from "./languages";
import { providerInfo } from "./providers";
import { modelTitle } from "./settings/modelRow";
import "./settings.css";
import { Dictation } from "./settings/Dictation";
import type { LanguageChoice } from "./settings/Dictation";
import { General, useLaunchAtLogin } from "./settings/General";
import { About, Help } from "./settings/HelpAbout";
import { History } from "./settings/History";
import type { HelpActions } from "./settings/HelpAbout";
import { Privacy } from "./settings/Privacy";
import { Speech } from "./settings/Speech";

/** The sidebar's sections, in order. */
export const SECTIONS = [
  { id: "general", name: "General" },
  { id: "dictation", name: "Dictation" },
  { id: "speech", name: "Speech" },
  { id: "privacy", name: "Privacy" },
  { id: "history", name: "History" },
  { id: "help", name: "Help" },
  { id: "about", name: "About" },
] as const;

export type SectionId = (typeof SECTIONS)[number]["id"];

/** Where the last section opened is remembered (this window's storage only). */
export const SECTION_KEY = "vocet.settings.section";

/** How often the models are read while one downloads, and the engine while it loads. */
export const POLL_MS = 500;

function rememberedSection(): SectionId {
  try {
    const stored = window.localStorage.getItem(SECTION_KEY);
    const found = SECTIONS.find((section) => section.id === stored);
    return found === undefined ? "general" : found.id;
  } catch {
    return "general";
  }
}

function remember(section: SectionId) {
  try {
    window.localStorage.setItem(SECTION_KEY, section);
  } catch {
    // Not remembered; Settings still works.
  }
}

/** The on-device catalogue, read again while a download runs. */
export function useModels() {
  const [models, setModels] = useState<LocalModelInfo_Serialize[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the newest reply counts.
  const read = useRef(0);
  const refresh = useCallback(async () => {
    read.current += 1;
    const run = read.current;
    try {
      const result = await commands.listLocalModels();
      if (run !== read.current) return;
      if (result.status === "error") {
        setError(messageOf(result.error));
        return;
      }
      setError(null);
      setModels(result.data);
    } catch {
      if (run === read.current) setError(BROKEN);
    }
  }, []);
  useEffect(() => {
    // The first read; a timer, so no state changes inside the effect itself.
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [refresh]);
  const downloading = models?.some((model) => model.state === "downloading") ?? false;
  useEffect(() => {
    if (!downloading) return;
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [downloading, refresh]);
  return { models, error, refresh };
}

/** The engine in use, read again while it loads. */
export function useEngineStatus() {
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const refresh = useCallback(async () => {
    try {
      const result = await commands.getEngineStatus();
      if (result.status === "ok") setEngine(result.data);
    } catch {
      // Keep what is shown.
    }
  }, []);
  useEffect(() => {
    // The first read; a timer, so no state changes inside the effect itself.
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [refresh]);
  const loading = engine?.state === "loading";
  useEffect(() => {
    if (!loading) return;
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [loading, refresh]);
  return { engine, refresh };
}

/** The languages the engine in use offers (U2). */
function languageChoice(
  settings: Settings_Serialize,
  models: LocalModelInfo_Serialize[] | null,
): LanguageChoice {
  if (settings.transcription === "cloud") {
    return {
      supported: cloudLanguages(settings.provider),
      engineName: providerInfo(settings.provider).name,
      englishOnly: false,
    };
  }
  const model = models?.find((candidate) => candidate.id === settings.local_model);
  // Before the list has been read, nothing is filtered or flagged.
  if (model === undefined) {
    return { supported: cloudLanguages(settings.provider), engineName: null, englishOnly: false };
  }
  const englishOnly = model.language_codes.join() === "en";
  return {
    supported: model.language_codes,
    engineName: `the ${modelTitle(model)} model`,
    englishOnly,
  };
}

interface Props extends HelpActions {
  /** Open at this section (the tray's fix rows); `id` makes a repeated ask count. */
  open?: { section: SectionId; id: number } | null;
  /**
   * The ask was followed. The app forgets it, so coming back from Troubleshoot
   * or Report (which remounts Settings) can't follow it again (#277).
   */
  onOpened?: () => void;
}

/**
 * The settings window (U6): a sidebar of sections and a scrolling pane of
 * grouped rows. Every control saves on change through a command that writes
 * only its own setting, so there is no Save button and no form whose older
 * values could overwrite another control's; operations with a lifecycle of
 * their own keep their buttons (Download, Test & Save, the dictation
 * key's recorder, Remove key). The API key is never loaded back.
 */
export function Settings({ open = null, onOpened, ...help }: Props) {
  const [settings, setSettings] = useState<Settings_Serialize | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [section, setSection] = useState<SectionId>(rememberedSection);
  const { models, error: modelsError, refresh: refreshModels } = useModels();
  const { engine, refresh: refreshEngine } = useEngineStatus();
  // The first launch after the rename says something about the Keychain (U1);
  // Speech shows it, and only with a key saved.
  const [launchNotice, setLaunchNotice] = useState<string | null>(null);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const result = await commands.launchNotice();
          if (result.status === "ok") setLaunchNotice(result.data);
        } catch {
          // No notice is the same as nothing to say.
        }
      })();
    }, 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, []);
  const [sheetOffered, setSheetOffered] = useState(false);
  const offerSheet = useCallback(() => {
    setSheetOffered(true);
  }, []);
  const launch = useLaunchAtLogin(settings?.launch_at_login ?? false);
  const tabs = useRef<Partial<Record<SectionId, HTMLButtonElement | null>>>({});

  const choose = useCallback((next: SectionId) => {
    setSection(next);
  }, []);

  // A new ask from the tray opens its section (state from props, during render).
  const [seenOpen, setSeenOpen] = useState<number | null>(null);
  if (open !== null && open.id !== seenOpen) {
    setSeenOpen(open.id);
    setSection(open.section);
  }
  useEffect(() => {
    remember(section);
  }, [section]);
  useEffect(() => {
    if (open !== null) onOpened?.();
  }, [open, onOpened]);

  // The window is hidden, not closed, so it outlives what it loaded:
  // whenever it comes back, read the settings again (the login item may
  // have changed in System Settings meanwhile).
  useEffect(() => {
    const state = { cancelled: false };
    const load = async () => {
      try {
        const result = await commands.getSettings();
        if (state.cancelled) return;
        if (result.status === "error") {
          setLoadError(messageOf(result.error));
          return;
        }
        setLoadError(null);
        setSettings(result.data);
      } catch {
        if (!state.cancelled) setLoadError(BROKEN);
      }
    };
    void load();
    const onShow = () => {
      if (document.visibilityState === "hidden") return;
      void load();
      void refreshEngine();
    };
    window.addEventListener("focus", onShow);
    document.addEventListener("visibilitychange", onShow);
    return () => {
      state.cancelled = true;
      window.removeEventListener("focus", onShow);
      document.removeEventListener("visibilitychange", onShow);
    };
  }, [refreshEngine]);

  function onSidebarKey(event: KeyboardEvent<HTMLDivElement>) {
    const index = SECTIONS.findIndex((candidate) => candidate.id === section);
    const target =
      event.key === "ArrowDown"
        ? SECTIONS[(index + 1) % SECTIONS.length]
        : event.key === "ArrowUp"
          ? SECTIONS[(index - 1 + SECTIONS.length) % SECTIONS.length]
          : event.key === "Home"
            ? SECTIONS[0]
            : event.key === "End"
              ? SECTIONS[SECTIONS.length - 1]
              : undefined;
    if (target === undefined) return;
    event.preventDefault();
    choose(target.id);
    tabs.current[target.id]?.focus();
  }

  const current = SECTIONS.find((candidate) => candidate.id === section) ?? SECTIONS[0];

  let pane;
  if (loadError !== null) {
    pane = (
      <p role="alert" className="notice error">
        Couldn&apos;t load the settings: {loadError}
      </p>
    );
  } else if (settings === null) {
    pane = <p>Loading…</p>;
  } else {
    switch (current.id) {
      case "general":
        pane = <General launch={launch} />;
        break;
      case "dictation":
        pane = (
          <Dictation
            settings={settings}
            onStored={setSettings}
            languages={languageChoice(settings, models)}
            onOpenSpeech={() => {
              choose("speech");
            }}
          />
        );
        break;
      case "speech":
        pane = (
          <Speech
            settings={settings}
            onStored={setSettings}
            models={models}
            modelsError={modelsError}
            refreshModels={refreshModels}
            engine={engine}
            refreshEngine={refreshEngine}
            launchNotice={launchNotice}
            sheetOffered={sheetOffered}
            onSheetOffered={offerSheet}
          />
        );
        break;
      case "privacy":
        pane = <Privacy settings={settings} onStored={setSettings} />;
        break;
      case "history":
        pane = <History settings={settings} onStored={setSettings} />;
        break;
      case "help":
        pane = <Help {...help} />;
        break;
      case "about":
        pane = <About />;
        break;
    }
  }

  return (
    <main className="settings-window">
      <div
        className="settings-sidebar"
        role="tablist"
        aria-label="Settings sections"
        aria-orientation="vertical"
        onKeyDown={onSidebarKey}
      >
        {SECTIONS.map((candidate) => (
          <button
            key={candidate.id}
            ref={(element) => {
              tabs.current[candidate.id] = element;
            }}
            type="button"
            role="tab"
            id={`tab-${candidate.id}`}
            aria-selected={candidate.id === section}
            aria-controls="settings-pane"
            tabIndex={candidate.id === section ? 0 : -1}
            className={candidate.id === section ? "selected" : undefined}
            onClick={() => {
              choose(candidate.id);
            }}
          >
            {candidate.name}
          </button>
        ))}
      </div>
      <div
        className="settings-pane"
        id="settings-pane"
        role="tabpanel"
        aria-labelledby={`tab-${current.id}`}
        tabIndex={0}
      >
        <h1>{current.name}</h1>
        {pane}
      </div>
    </main>
  );
}
