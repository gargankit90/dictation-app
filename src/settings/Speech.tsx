import { useState } from "react";
import { commands } from "../bindings";
import type {
  EngineStatus,
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
} from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { providerInfo } from "../providers";
import { modelCaption, modelRow, modelTitle } from "./modelRow";
import type { ModelAction } from "./modelRow";
import { OwnKeyPanel, useOwnKey } from "./ownKey";
import { Chip, Group, Row, SaveStatus, useSave } from "./ui";

export { CONNECTED, KEY_NOT_SAVED } from "./ownKey";

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
}

/** What a model action's button says. */
function actionLabel(action: ModelAction): string {
  switch (action) {
    case "download":
      return "Download";
    case "cancel":
      return "Cancel";
    case "resume":
      return "Resume";
    case "retry":
    case "retry-load":
      return "Retry";
    case "use":
      return "Use this";
    case "delete":
      return "Delete";
  }
}

/** `record` without `key`. */
function without(record: Record<string, string>, key: string): Record<string, string> {
  return Object.fromEntries(Object.entries(record).filter(([name]) => name !== key));
}

/**
 * Speech: one row per engine, exactly one of them Active. On-device models
 * follow the state machine in `modelRow`; "Your own key" opens a panel to
 * pick the provider and Test & Save a key (saved only if the test passes).
 */
export function Speech({
  settings,
  onStored,
  models,
  modelsError,
  refreshModels,
  engine,
  refreshEngine,
}: Props) {
  const onDevice = settings.transcription === "local";
  const [switching, setSwitching] = useState<string | null>(null);
  const [loadErrors, setLoadErrors] = useState<Record<string, string>>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const cloud = useSave();
  // The own-key panel.
  const [panelOpen, setPanelOpen] = useState(!onDevice);

  async function reread() {
    try {
      const result = await commands.getSettings();
      if (result.status === "ok") onStored(result.data);
    } catch {
      // What is shown stays; the next change reads again.
    }
  }

  async function modelAction(action: ModelAction, model: LocalModelInfo_Serialize) {
    setActionError(null);
    setMenuOpen(null);
    if (action === "use" || action === "retry-load") {
      setSwitching(model.id);
      setLoadErrors((now) => without(now, model.id));
      try {
        const result = await commands.useLocalModel(model.id);
        if (result.status === "error") {
          setLoadErrors((now) => ({ ...now, [model.id]: messageOf(result.error) }));
        }
      } catch {
        setLoadErrors((now) => ({ ...now, [model.id]: BROKEN }));
      }
      await reread();
      await refreshEngine();
      setSwitching(null);
      return;
    }
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

  async function switchToCloud(which: Provider) {
    await cloud.track(async () => {
      const result = await commands.useCloudEngine(which);
      await reread();
      await refreshEngine();
      return result.status === "error" ? messageOf(result.error) : null;
    });
  }

  const ownKey = useOwnKey({ settings, onDevice, switchToCloud, refreshEngine });
  const { provider, keyStatus } = ownKey;
  const info = providerInfo(provider);
  // While a model loads after "Use this", the own key stays the engine in use.
  const cloudActive = !onDevice;
  // Active means it can dictate: with no saved key it can't (Design Lead,
  // round 1). It becomes ✓ Active once Test & Save has saved one.
  const stored = provider === settings.provider ? keyStatus.state : "checking";
  const ownKeyChip =
    stored === "saved" ? (
      <Chip tone="ready">✓ Active</Chip>
    ) : stored === "checking" ? (
      <Chip tone="plain">Checking…</Chip>
    ) : (
      <Chip tone="attention">! Needs a key</Chip>
    );

  return (
    <>
      <Group title="Speech engine">
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
            const row = modelRow(model, {
              chosen: onDevice && settings.local_model === model.id,
              engine,
              switching: switching === model.id,
              loadError: loadErrors[model.id] ?? null,
            });
            const labelId = `model-${model.id}-label`;
            const error =
              loadErrors[model.id] ??
              (row.chip.text === "! Couldn't load" && engine?.message ? engine.message : null);
            return (
              <Row
                key={model.id}
                label={modelTitle(model)}
                labelId={labelId}
                caption={modelCaption(model)}
                below={
                  <>
                    {row.progress === null ? null : (
                      <progress
                        aria-label={`${modelTitle(model)} download`}
                        max={row.progress.max}
                        value={row.progress.value}
                      />
                    )}
                    {error === null ? null : (
                      <p role="alert" className="error row-error">
                        {error}
                      </p>
                    )}
                    {model.error === null ? null : <p className="hint row-error">{model.error}</p>}
                  </>
                }
              >
                <span aria-live="polite">
                  <Chip tone={row.chip.tone}>{row.chip.text}</Chip>
                </span>
                {row.actions.map((action) => (
                  <button
                    key={action}
                    type="button"
                    className={action === "delete" ? "destructive" : undefined}
                    aria-describedby={labelId}
                    disabled={switching !== null && (action === "use" || action === "retry-load")}
                    onClick={() => void modelAction(action, model)}
                  >
                    {actionLabel(action)}
                  </button>
                ))}
                {row.canDelete && (
                  <span className="row-menu">
                    <button
                      type="button"
                      className="menu-button"
                      aria-label={`More actions for ${modelTitle(model)}`}
                      aria-haspopup="menu"
                      aria-expanded={menuOpen === model.id}
                      onClick={() => {
                        setMenuOpen((open) => (open === model.id ? null : model.id));
                      }}
                    >
                      …
                    </button>
                    {menuOpen === model.id && (
                      <span role="menu" className="row-popover">
                        <button
                          type="button"
                          role="menuitem"
                          className="destructive"
                          onClick={() => void modelAction("delete", model)}
                        >
                          Delete {modelTitle(model)}
                        </button>
                      </span>
                    )}
                  </span>
                )}
              </Row>
            );
          })
        )}
        <Row
          label="Your own key"
          labelId="own-key-label"
          caption={`${info.name} · needs internet; speech is sent to ${info.name}`}
          below={
            <>
              <SaveStatus save={cloud.save} />
              {panelOpen && <OwnKeyPanel ownKey={ownKey} />}
            </>
          }
        >
          {cloudActive ? (
            ownKeyChip
          ) : (
            <button
              type="button"
              aria-describedby="own-key-label"
              disabled={switching !== null || cloud.save.state === "saving"}
              onClick={() => void switchToCloud(provider)}
            >
              Use this
            </button>
          )}
          <button
            type="button"
            aria-expanded={panelOpen}
            aria-controls="own-key-panel"
            onClick={() => {
              setPanelOpen((open) => !open);
            }}
          >
            {panelOpen ? "Hide" : "Set up…"}
          </button>
        </Row>
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
    </>
  );
}
