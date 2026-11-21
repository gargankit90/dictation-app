import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { commands } from "../bindings";
import type { EngineStatus, LocalModelInfo_Serialize, Settings_Serialize } from "../bindings";
import { BROKEN, messageOf } from "../messages";
import {
  downloadingText,
  megabytes,
  LOCAL_CAPTION,
  LOCAL_TITLE,
  modelDetails,
  modelRow,
  cloudCaption,
} from "./modelRow";
import type { ModelAction } from "./modelRow";
import { useCleanupModels, withCleanup } from "./CleanupModel";
import { ConnectSheet, useConnection } from "./connection";
import { Chip, Group, Row, SaveStatus, useSave } from "./ui";

export { CONNECTED, KEY_NOT_SAVED } from "./connection";

interface Props {
  settings: Settings_Serialize;
  onStored: (settings: Settings_Serialize) => void;
  models: LocalModelInfo_Serialize[] | null;
  modelsError: string | null;
  /** Reads the models again (after a download action). */
  refreshModels: () => Promise<void>;
  engine: EngineStatus | null;
  /** Reads the engine's status again. */
  refreshEngine: () => Promise<void>;
  /** The sheet already opened by itself since Settings opened. */
  sheetOffered?: boolean;
  onSheetOffered?: () => void;
}

/** What a model action's button says. */
function actionLabel(action: ModelAction, model: LocalModelInfo_Serialize): string {
  switch (action) {
    case "download":
      return `Download · ${megabytes(model.size_bytes ?? 0)}`;
    case "cancel":
      // It keeps what came down, and Resume continues it.
      return "Pause";
    case "resume":
      return "Resume";
    case "retry":
    case "retry-load":
      return "Retry";
  }
}

/** `record` without `key`. */
function without(record: Record<string, string>, key: string): Record<string, string> {
  return Object.fromEntries(Object.entries(record).filter(([name]) => name !== key));
}

/** One row of the pick-one list: choosing the row chooses the engine. */
function EngineRow({
  id,
  name,
  caption,
  checked,
  blockedBecause,
  disabled,
  onPick,
  children,
  below,
}: {
  id: string;
  name: string;
  caption: string;
  checked: boolean;
  /** Why it can't be chosen, for assistive technology and the pointer. */
  blockedBecause: string | null;
  disabled: boolean;
  onPick: () => void;
  children?: ReactNode;
  below?: ReactNode;
}) {
  const reasonId = `${id}-reason`;
  return (
    <div className="settings-row engine-row">
      <div className="row-main">
        <label className="engine-pick" title={blockedBecause ?? undefined}>
          <input
            id={id}
            type="radio"
            name="speech-engine"
            checked={checked}
            disabled={disabled}
            aria-label={name}
            aria-description={caption}
            aria-describedby={blockedBecause === null ? undefined : reasonId}
            onChange={() => {
              if (!disabled) onPick();
            }}
          />
          <span className="row-text">
            <span className="row-label">{name}</span>
            <span className="row-caption">{caption}</span>
          </span>
        </label>
        <div className="row-control">{children}</div>
      </div>
      {blockedBecause === null ? null : (
        <span id={reasonId} className="visually-hidden">
          {blockedBecause}
        </span>
      )}
      {below}
    </div>
  );
}

/**
 * Speech (U12): one pick-one list of engines. Selecting a row chooses the
 * engine; a model that isn't ready can't be selected and offers its download,
 * and "Your own key" needs a key that works first, from its sheet (a key test
 * saves only if it passes).
 */
export function Speech({
  settings,
  onStored,
  models,
  modelsError,
  refreshModels,
  engine,
  refreshEngine,
  sheetOffered = false,
  onSheetOffered,
}: Props) {
  const onDevice = settings.transcription === "local";
  // Gemma: part of the "On this Mac" row's one download (Q4).
  const cleanup = useCleanupModels(true)?.[0];
  const [switching, setSwitching] = useState<string | null>(null);
  const [loadErrors, setLoadErrors] = useState<Record<string, string>>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const cloud = useSave();
  const [sheetOpen, setSheetOpen] = useState(false);
  // Selecting OpenRouter with no key waits here for a connection that works.
  const switchOnceConnected = useRef(false);
  // Whatever gets the sheet's focus back when it closes.
  const sheetOpener = useRef<HTMLElement | null>(null);
  useEffect(
    () => () => {
      switchOnceConnected.current = false;
    },
    [],
  );

  async function reread() {
    try {
      const result = await commands.getSettings();
      if (result.status === "ok") onStored(result.data);
    } catch {
      // What is shown stays; the next change reads again.
    }
  }

  async function chooseModel(model: LocalModelInfo_Serialize) {
    setActionError(null);
    setMenuOpen(null);
    setSwitching(model.id);
    setLoadErrors((now) => without(now, model.id));
    try {
      const result = await commands.setTranscription("local");
      if (result.status === "error") {
        setLoadErrors((now) => ({ ...now, [model.id]: messageOf(result.error) }));
      }
    } catch {
      setLoadErrors((now) => ({ ...now, [model.id]: BROKEN }));
    }
    await reread();
    await refreshEngine();
    setSwitching(null);
  }

  async function modelAction(action: ModelAction | "delete", model: LocalModelInfo_Serialize) {
    if (action === "retry-load") {
      await chooseModel(model);
      return;
    }
    setActionError(null);
    setMenuOpen(null);
    try {
      const call =
        action === "cancel"
          ? commands.cancelLocalModelDownload(model.id)
          : action === "delete"
            ? commands.deleteLocalModel(model.id)
            : commands.downloadLocalModel(model.id);
      const result = await call;
      if (result.status === "error") setActionError(messageOf(result.error));
    } catch {
      setActionError(BROKEN);
    }
    if (action === "delete") setLoadErrors((now) => without(now, model.id));
    await refreshModels();
    await refreshEngine();
  }

  async function switchToCloud() {
    await cloud.track(async () => {
      const result = await commands.setTranscription("cloud");
      await reread();
      await refreshEngine();
      return result.status === "error" ? messageOf(result.error) : null;
    });
  }

  const connection = useConnection({
    refreshEngine,
    onConnected: async () => {
      const waiting = switchOnceConnected.current;
      switchOnceConnected.current = false;
      if (waiting) await switchToCloud();
    },
  });
  const { status } = connection;
  const cloudActive = !onDevice;
  const stored = status.state;

  // The own key is the engine and needs a key: the sheet opens by itself, once
  // and not again on every return to Speech (Design Lead item 6). State from
  // props, during render.
  const [offeredSheet, setOfferedSheet] = useState(sheetOffered);
  if (!offeredSheet && cloudActive && stored === "none") {
    setOfferedSheet(true);
    setSheetOpen(true);
  }
  useEffect(() => {
    if (offeredSheet) onSheetOffered?.();
  }, [offeredSheet, onSheetOffered]);

  function openSheet() {
    sheetOpener.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setSheetOpen(true);
  }

  async function closeSheet() {
    // A key that is being saved keeps the sheet: it shows how that ends.
    if (!(await connection.canLeave())) return;
    switchOnceConnected.current = false;
    connection.clearNotice();
    setSheetOpen(false);
    sheetOpener.current?.focus();
    sheetOpener.current = null;
  }

  async function chooseCloud() {
    setActionError(null);
    let saved = false;
    try {
      const result = await commands.savedKeyHint();
      saved = result.status === "ok" && result.data !== null;
    } catch {
      // Treated as no key: the sheet says what is wrong.
    }
    if (saved) {
      await switchToCloud();
      return;
    }
    switchOnceConnected.current = true;
    openSheet();
  }

  const busy = switching !== null || cloud.save.state === "saving";
  const switchingName = models?.find((model) => model.id === switching);
  const waitReason = switchingName === undefined ? null : `Wait until ${LOCAL_TITLE} has loaded.`;

  return (
    <>
      <Group title="Speech engine">
        <div role="radiogroup" aria-label="Speech engine">
          <EngineRow
            id="engine-openrouter"
            name="OpenRouter"
            caption={cloudCaption(status.state === "connected")}
            checked={cloudActive && switching === null}
            blockedBecause={busy && !cloudActive ? waitReason : null}
            disabled={busy}
            onPick={() => void chooseCloud()}
            below={<SaveStatus save={cloud.save} />}
          >
            {!cloudActive ? null : stored === "checking" ? (
              <Chip tone="plain">Checking…</Chip>
            ) : stored === "connected" ? null : (
              <Chip tone="attention">! Not connected</Chip>
            )}
            <button type="button" aria-description="OpenRouter" onClick={openSheet}>
              {status.state === "connected" ? "Manage…" : "Connect…"}
            </button>
          </EngineRow>
          {models === null ? (
            modelsError === null ? (
              <Row label="On this Mac" caption="Loading the models…" />
            ) : (
              <Row
                label="On this Mac"
                below={
                  <p role="alert" className="error row-error">
                    Couldn&apos;t list the models: {modelsError}
                  </p>
                }
              />
            )
          ) : (
            models.map((model) => {
              const chosen = onDevice && model.default;
              // One row, one download: speech then cleanup (Q4).
              const shown = withCleanup(model, cleanup);
              const row = modelRow(shown, {
                chosen,
                engine,
                switching: switching === model.id,
                loadError: loadErrors[model.id] ?? null,
              });
              const title = LOCAL_TITLE;
              const error =
                loadErrors[model.id] ??
                (row.chip?.text === "! Couldn't load" && engine?.message ? engine.message : null);
              // A model still downloading can already be the chosen one.
              const checked = switching === null ? chosen : switching === model.id;
              const reason = row.blockedBecause ?? (busy && !checked ? waitReason : null);
              return (
                <EngineRow
                  key={model.id}
                  id={`engine-${model.id}`}
                  name={title}
                  caption={shown.state === "downloading" ? downloadingText(shown) : LOCAL_CAPTION}
                  checked={checked}
                  blockedBecause={reason}
                  disabled={row.blockedBecause !== null || busy}
                  onPick={() => void chooseModel(model)}
                  below={
                    <>
                      {row.progress === null ? null : (
                        <progress
                          aria-label={`${title} download`}
                          max={row.progress.max}
                          value={row.progress.value}
                        />
                      )}
                      {error === null ? null : (
                        <p role="alert" className="error row-error">
                          {error}
                        </p>
                      )}
                      {model.error === null ? null : (
                        <p className="hint row-error">{model.error}</p>
                      )}
                      {reason === null || row.actions.includes("download") ? null : (
                        <p className="hint row-error" data-testid="blocked-reason">
                          {reason}
                        </p>
                      )}
                      {/* In the row's flow, under its text: it never covers it. */}
                      {menuOpen === model.id && (
                        <span role="menu" className="row-popover">
                          <span role="presentation" className="row-popover-info">
                            {modelDetails(model)}
                          </span>
                          {row.canDelete ? (
                            <button
                              type="button"
                              role="menuitem"
                              className="destructive"
                              onClick={() => void modelAction("delete", model)}
                            >
                              Delete model
                            </button>
                          ) : (
                            <span role="presentation" className="row-popover-info">
                              In use. Choose another engine to delete it.
                            </span>
                          )}
                        </span>
                      )}
                    </>
                  }
                >
                  {row.chip === null ? null : (
                    <span aria-live="polite">
                      <Chip tone={row.chip.tone}>{row.chip.text}</Chip>
                    </span>
                  )}
                  {row.actions.map((action) => (
                    <button
                      key={action}
                      type="button"
                      aria-description={title}
                      disabled={switching !== null && action === "retry-load"}
                      onClick={() => void modelAction(action, model)}
                    >
                      {actionLabel(action, shown)}
                    </button>
                  ))}
                  {row.hasMenu && (
                    <span className="row-menu">
                      <button
                        type="button"
                        className="menu-button"
                        aria-label={`More actions for ${title}`}
                        aria-haspopup="menu"
                        aria-expanded={menuOpen === model.id}
                        onClick={() => {
                          setMenuOpen((open) => (open === model.id ? null : model.id));
                        }}
                      >
                        …
                      </button>
                    </span>
                  )}
                </EngineRow>
              );
            })
          )}
        </div>
      </Group>
      {actionError !== null && (
        <p role="alert" className="error">
          {actionError}
        </p>
      )}
      {models?.some((model) => model.state === "downloading") && (
        <p className="hint">
          One model downloads at a time. Downloads go on while Settings is closed.
        </p>
      )}
      {sheetOpen && (
        <ConnectSheet
          connection={connection}
          onDone={() => {
            void closeSheet();
          }}
        />
      )}
    </>
  );
}
