import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "../bindings";
import type { Settings_Serialize } from "../bindings";
import { HotkeyRecorder } from "../HotkeyRecorder";
import { BROKEN, messageOf } from "../keyTest";
import { LanguagePicker } from "../LanguagePicker";
import { SILENT_LEVELS, pushLevel } from "../overlay/view";
import { saveHotkey, setHotkeyRecording } from "../saveHotkey";
import { MIC_POLL_MS, MicMeter } from "./MicMeter";
import { Group, Row, SaveStatus, useSave } from "./ui";

/** How long Test microphone listens. */
export const MIC_TEST_MS = 5_000;
export { MIC_POLL_MS } from "./MicMeter";
/** Louder than this, in dBFS, is sound (`NO_SOUND_THRESHOLD_DBFS` in `dictation-core`). */
export const SOUND_DBFS = -50;
/** How often the Shortcut row asks whether the hotkey still waits. */
export const HOTKEY_NOTICE_POLL_MS = 2_000;

export interface LanguageChoice {
  /** The codes the engine in use offers. */
  supported: readonly string[];
  /** The engine's name, in plain words, or `null` if not known yet. */
  engineName: string | null;
  /** The engine is the English-only model. */
  englishOnly: boolean;
}

interface Props {
  settings: Settings_Serialize;
  /** The app's settings after a change (what Settings shows from now). */
  onStored: (settings: Settings_Serialize) => void;
  languages: LanguageChoice;
  /** Opens the Speech section (to pick a multilingual engine). */
  onOpenSpeech: () => void;
}

type MicTest =
  | { state: "idle" }
  | { state: "starting" }
  | { state: "listening"; levels: number[]; heard: boolean }
  | { state: "done"; heard: boolean }
  | { state: "error"; message: string };

/**
 * Dictation: the shortcut (K1's recorder, applied at once), the language
 * (saved on change) and the microphone with a 5 s level test.
 */
export function Dictation({ settings, onStored, languages, onOpenSpeech }: Props) {
  const hotkey = useSave();
  const language = useSave();
  const [mic, setMic] = useState<MicTest>({ state: "idle" });
  const [device, setDevice] = useState<string | null>(null);
  // Bumped by every test, so an old one's timers stop.
  const micRun = useRef(0);
  const [hotkeyNotice, setHotkeyNotice] = useState<string | null>(null);

  const readHotkeyNotice = useCallback(async () => {
    try {
      const result = await commands.getHotkeyNotice();
      if (result.status === "ok") setHotkeyNotice(result.data);
    } catch {
      // What is shown stays; the next read tries again.
    }
  }, []);

  // A hotkey waiting for Accessibility isn't a failed save, so no save says so:
  // the row does, until the key listens. That happens by itself once it's
  // allowed in System Settings, so the row keeps asking.
  useEffect(() => {
    const first = window.setTimeout(() => void readHotkeyNotice(), 0);
    const timer = window.setInterval(() => void readHotkeyNotice(), HOTKEY_NOTICE_POLL_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [readHotkeyNotice]);

  useEffect(
    () => () => {
      micRun.current += 1;
      void commands.stopMicrophoneTest().catch(() => undefined);
    },
    [],
  );

  async function recordHotkey(spec: string): Promise<string | null> {
    const outcome: { problem: string | null } = { problem: null };
    await hotkey.track(async () => {
      const result = await saveHotkey(spec);
      if (result.settings !== null) onStored(result.settings);
      outcome.problem = result.error;
      // The recorder shows its own message; the row shows only "Saved".
      return result.error === null ? null : "";
    });
    if (outcome.problem !== null) hotkey.clear();
    void readHotkeyNotice();
    return outcome.problem;
  }

  async function changeLanguage(code: string) {
    await language.track(async () => {
      const result = await commands.setLanguage(code === "" ? null : code);
      const reread = await commands.getSettings();
      if (reread.status === "ok") onStored(reread.data);
      return result.status === "error" ? messageOf(result.error) : null;
    });
  }

  async function testMicrophone() {
    micRun.current += 1;
    const run = micRun.current;
    setMic({ state: "starting" });
    try {
      const started = await commands.startMicrophoneTest();
      if (run !== micRun.current) return;
      if (started.status === "error") {
        setMic({ state: "error", message: messageOf(started.error) });
        return;
      }
    } catch {
      setMic({ state: "error", message: BROKEN });
      return;
    }
    let levels = [...SILENT_LEVELS];
    let heard = false;
    setMic({ state: "listening", levels, heard });
    const began = Date.now();
    const poll = window.setInterval(() => {
      void (async () => {
        if (run !== micRun.current) {
          window.clearInterval(poll);
          return;
        }
        if (Date.now() - began >= MIC_TEST_MS) {
          window.clearInterval(poll);
          micRun.current += 1;
          await commands.stopMicrophoneTest().catch(() => undefined);
          setMic({ state: "done", heard });
          return;
        }
        try {
          const reading = await commands.readMicrophoneTest();
          if (run !== micRun.current || reading.status !== "ok") return;
          if (reading.data.device !== null) setDevice(reading.data.device);
          const level = reading.data.level;
          if (level === null) return;
          if (level > SOUND_DBFS) heard = true;
          levels = pushLevel(levels, level);
          setMic({ state: "listening", levels, heard });
        } catch {
          // The next reading tries again.
        }
      })();
    }, MIC_POLL_MS);
  }

  const micBusy = mic.state === "starting" || mic.state === "listening";

  return (
    <>
      <Group>
        <Row
          label="Shortcut"
          labelId="hotkey-label"
          caption="Hold it to talk, release to paste. It works as soon as you record it."
          below={
            <>
              <SaveStatus save={hotkey.save} />
              {hotkeyNotice !== null && (
                <p role="status" className="notice attention">
                  {hotkeyNotice}
                </p>
              )}
            </>
          }
        >
          <HotkeyRecorder
            value={settings.hotkey}
            onChange={recordHotkey}
            onRecording={setHotkeyRecording}
          />
        </Row>
        <Row
          label={<label htmlFor="language">Language</label>}
          caption={
            languages.englishOnly ? (
              <>
                The English model understands English only. For Hindi and other languages,{" "}
                <button type="button" className="link" onClick={onOpenSpeech}>
                  choose a multilingual engine in Speech.
                </button>
              </>
            ) : (
              "Choose the language you speak, or let Vocet detect it."
            )
          }
          captionId="language-hint"
          below={<SaveStatus save={language.save} id="language-status" />}
        >
          <LanguagePicker
            id="language"
            value={settings.language ?? ""}
            onChange={(code) => void changeLanguage(code)}
            supported={languages.supported}
            engineName={languages.engineName}
            describedBy="language-hint language-status"
            invalid={language.save.state === "error"}
          />
        </Row>
      </Group>
      <Group title="Microphone">
        <Row
          label={device ?? "The Mac's default input"}
          labelId="mic-label"
          caption="Vocet uses the input chosen in System Settings → Sound. Change it there."
          below={
            mic.state === "listening" ? (
              <div className="mic-test" role="status">
                <MicMeter levels={mic.levels} />
                <span>{mic.heard ? "Hearing you." : "Listening… say something."}</span>
              </div>
            ) : mic.state === "done" ? (
              mic.heard ? (
                <p role="status" className="row-note">
                  ✓ The microphone works.
                </p>
              ) : (
                <p role="alert" className="error row-error">
                  No sound. Check that the right input is chosen in System Settings → Sound and that
                  it isn&apos;t muted.
                </p>
              )
            ) : mic.state === "error" ? (
              <p role="alert" className="error row-error">
                {mic.message}
              </p>
            ) : null
          }
        >
          <button type="button" disabled={micBusy} onClick={() => void testMicrophone()}>
            {micBusy ? "Testing…" : "Test microphone"}
          </button>
        </Row>
      </Group>
    </>
  );
}
