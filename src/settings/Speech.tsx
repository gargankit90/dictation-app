import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "../bindings";
import type {
  EngineStatus,
  LocalModelInfo_Serialize,
  Provider,
  Settings_Serialize,
} from "../bindings";
import { BROKEN, messageOf } from "../keyTest";
import { PROVIDERS, providerInfo } from "../providers";
import { validateKey } from "../validation";
import { modelCaption, modelRow, modelTitle } from "./modelRow";
import type { ModelAction } from "./modelRow";
import { Chip, Group, Row, SaveStatus, useSave } from "./ui";

/** Shown after Test & Save, once the provider accepted the key and the Keychain kept it. */
export const CONNECTED = "✓ Connected · saved in Keychain";
/** A good key the Keychain wouldn't keep (U6 criterion 5). */
export const KEY_NOT_SAVED = "Key works, but couldn't be saved to your Keychain";

/** Where each provider's keys are made, and the steps to get one. */
const KEY_STEPS: Record<Provider, { link: "groq_keys" | "openai_keys"; site: string }> = {
  groq: { link: "groq_keys", site: "console.groq.com" },
  openai: { link: "openai_keys", site: "platform.openai.com" },
};

type KeyStatus =
  | { state: "checking" }
  | { state: "saved" }
  | { state: "none" }
  | { state: "error"; message: string };

type KeyNotice = { kind: "ok" | "error"; text: string } | null;

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

/**
 * The key operations' generation, bumped by every Test & Save, edit of
 * the key field, provider change and Remove key; a test writes its key
 * only if it is still the newest. In module scope, not in the component,
 * so leaving Speech and coming back can't reset it while an old test is
 * still waiting for the provider (review round 2).
 */
const keyRun = { current: 0 };

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
  const [provider, setProvider] = useState<Provider>(settings.provider);
  const [keyInput, setKeyInput] = useState("");
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ state: "checking" });
  const [keyNotice, setKeyNotice] = useState<KeyNotice>(null);
  const [keyBusy, setKeyBusy] = useState<"testing" | "saving" | "removing" | null>(null);
  // A key write is running; edits don't cancel it (it is already sent).
  const writing = useRef(false);
  // Only the newest key lookup counts.
  const keyCheck = useRef(0);

  // The panel follows the stored provider when it changes elsewhere.
  const [seenProvider, setSeenProvider] = useState(settings.provider);
  if (settings.provider !== seenProvider) {
    setSeenProvider(settings.provider);
    setProvider(settings.provider);
  }

  const checkKeySaved = useCallback(async (which: Provider) => {
    keyCheck.current += 1;
    const run = keyCheck.current;
    setKeyStatus({ state: "checking" });
    let status: KeyStatus;
    try {
      const result = await commands.hasApiKey(which);
      status =
        result.status === "error"
          ? { state: "error", message: messageOf(result.error) }
          : { state: result.data ? "saved" : "none" };
    } catch {
      status = { state: "error", message: BROKEN };
    }
    if (run === keyCheck.current) setKeyStatus(status);
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void checkKeySaved(provider), 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [provider, checkKeySaved]);

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

  function changeProvider(next: Provider) {
    keyRun.current += 1;
    setKeyBusy(null);
    setProvider(next);
    setKeyInput("");
    setKeyNotice(null);
    // Instant apply: while the own key is the engine, the provider is too.
    if (!onDevice) void switchToCloud(next);
  }

  async function testAndSave() {
    const typed = keyInput.trim();
    if (typed === "") {
      setKeyNotice({ kind: "error", text: "Paste your API key." });
      return;
    }
    const problem = validateKey(typed);
    if (problem !== null) {
      setKeyNotice({ kind: "error", text: problem });
      return;
    }
    keyRun.current += 1;
    const run = keyRun.current;
    const which = provider;
    setKeyNotice(null);
    setKeyBusy("testing");
    let notice: KeyNotice;
    try {
      const tested = await commands.testApiKey(which, typed);
      if (tested.status === "error") {
        notice = { kind: "error", text: tested.error.message };
      } else {
        // Only the newest Test & Save may write, and only if no edit,
        // removal or newer save came since it started: an obsolete test
        // must never restore a removed key or replace a newer one.
        if (run !== keyRun.current) return;
        // From here the write is committed: the field and buttons wait for it.
        writing.current = true;
        setKeyBusy("saving");
        const saved = await commands.setApiKey(which, typed).finally(() => {
          writing.current = false;
        });
        notice =
          saved.status === "error"
            ? { kind: "error", text: `${KEY_NOT_SAVED}: ${messageOf(saved.error)}` }
            : { kind: "ok", text: CONNECTED };
      }
    } catch {
      notice = { kind: "error", text: BROKEN };
    }
    if (run !== keyRun.current) return;
    setKeyBusy(null);
    setKeyNotice(notice);
    if (notice.kind === "ok") {
      setKeyInput("");
      keyCheck.current += 1; // any lookup still running predates this key
      setKeyStatus({ state: "saved" });
      await refreshEngine();
    }
  }

  async function removeKey() {
    keyRun.current += 1;
    const run = keyRun.current;
    setKeyNotice(null);
    setKeyBusy("removing");
    let notice: KeyNotice = null;
    try {
      const result = await commands.deleteApiKey(provider);
      if (result.status === "error") notice = { kind: "error", text: messageOf(result.error) };
    } catch {
      notice = { kind: "error", text: BROKEN };
    }
    if (run !== keyRun.current) return;
    setKeyBusy(null);
    setKeyNotice(notice ?? { kind: "ok", text: "Key removed from your Keychain." });
    void checkKeySaved(provider);
    await refreshEngine();
  }

  const info = providerInfo(provider);
  const steps = KEY_STEPS[provider];
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
              {panelOpen && (
                <div className="own-key-panel" id="own-key-panel">
                  <div className="panel-row">
                    <label htmlFor="provider">Provider</label>
                    <select
                      id="provider"
                      value={provider}
                      disabled={keyBusy === "saving"}
                      onChange={(event) => {
                        changeProvider(event.target.value as Provider);
                      }}
                    >
                      {PROVIDERS.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.name}
                        </option>
                      ))}
                    </select>
                  </div>
                  <ol className="key-steps">
                    <li>
                      Open{" "}
                      <button
                        type="button"
                        className="link"
                        onClick={() => {
                          void commands.openLink(steps.link).catch(() => undefined);
                        }}
                      >
                        {steps.site}
                      </button>{" "}
                      and sign in.
                    </li>
                    <li>Create an API key and copy it.</li>
                    <li>Paste it below and press Test &amp; Save.</li>
                  </ol>
                  <div className="panel-row">
                    <label htmlFor="api-key">{info.name} API key</label>
                    <input
                      id="api-key"
                      type="password"
                      value={keyInput}
                      autoComplete="off"
                      autoCapitalize="off"
                      autoCorrect="off"
                      spellCheck={false}
                      placeholder={
                        keyStatus.state === "saved"
                          ? "Paste a new key to replace it"
                          : "Paste your key"
                      }
                      aria-describedby="api-key-status"
                      aria-invalid={keyNotice?.kind === "error"}
                      readOnly={keyBusy === "saving"}
                      onChange={(event) => {
                        if (writing.current) return;
                        keyRun.current += 1;
                        setKeyBusy(null);
                        setKeyInput(event.target.value);
                        setKeyNotice(null);
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") void testAndSave();
                      }}
                    />
                  </div>
                  <p id="api-key-status" className="hint">
                    {keyStatus.state === "saved" ? (
                      <strong className="saved">A key is saved in your Keychain.</strong>
                    ) : keyStatus.state === "none" ? (
                      "No key saved yet."
                    ) : keyStatus.state === "checking" ? (
                      "Checking for a saved key…"
                    ) : (
                      <span role="alert" className="error">
                        Couldn&apos;t check for a saved key: {keyStatus.message}
                      </span>
                    )}{" "}
                    The key is never shown again.
                  </p>
                  <div className="row">
                    <button
                      type="button"
                      className="primary"
                      disabled={keyBusy !== null}
                      onClick={() => void testAndSave()}
                    >
                      {keyBusy === "testing"
                        ? "Testing…"
                        : keyBusy === "saving"
                          ? "Saving…"
                          : "Test & Save"}
                    </button>
                    {keyStatus.state === "saved" && (
                      <button
                        type="button"
                        className="destructive"
                        disabled={keyBusy !== null}
                        onClick={() => void removeKey()}
                      >
                        {keyBusy === "removing" ? "Removing…" : "Remove key"}
                      </button>
                    )}
                    {keyStatus.state === "error" && (
                      <button type="button" onClick={() => void checkKeySaved(provider)}>
                        Retry
                      </button>
                    )}
                  </div>
                  {keyNotice !== null && (
                    <p
                      role={keyNotice.kind === "error" ? "alert" : "status"}
                      className={`notice ${keyNotice.kind}`}
                    >
                      {keyNotice.text}
                    </p>
                  )}
                </div>
              )}
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
