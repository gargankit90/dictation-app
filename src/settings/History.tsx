import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent, RefObject } from "react";
import { commands } from "../bindings";
import type { HistoryList, Settings_Serialize } from "../bindings";
import { keycapLabel } from "../hotkeyKeys";
import { BROKEN, messageOf } from "../keyTest";
import { Chip, Group, Row, SAVED_FOR_MS, SaveStatus, useSave } from "./ui";

/** How often the list is read while this section shows, so a dictation appears. */
export const HISTORY_POLL_MS = 1_000;

/** The choices of "Keep recent dictations" (the app refuses any other). */
const LIMITS = [
  { value: 0, label: "Off" },
  { value: 10, label: "10" },
  { value: 25, label: "25" },
  { value: 50, label: "50" },
  { value: 100, label: "100" },
];

function twoDigits(value: number): string {
  return String(value).padStart(2, "0");
}

function startOfDay(time: Date): number {
  return new Date(time.getFullYear(), time.getMonth(), time.getDate()).getTime();
}

/** "Just now", "2 min ago", "Today 14:05", "Yesterday 14:05", or the date. */
export function relativeTime(createdAt: number, now: number): string {
  const minutes = Math.floor((now - createdAt) / 60_000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${String(minutes)} min ago`;
  const then = new Date(createdAt);
  const today = new Date(now);
  const clock = `${twoDigits(then.getHours())}:${twoDigits(then.getMinutes())}`;
  // By calendar days, which daylight saving can make other than 24 hours.
  const days = Math.round((startOfDay(today) - startOfDay(then)) / 86_400_000);
  if (days === 0) return `Today ${clock}`;
  if (days === 1) return `Yesterday ${clock}`;
  const date = then.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    ...(then.getFullYear() === today.getFullYear() ? {} : { year: "numeric" }),
  });
  return `${date} ${clock}`;
}

/**
 * What a row shows. The stylesheet cuts it to two lines; when line breaks make
 * the cut here instead, the last full stop goes, so it doesn't read "tomorrow....".
 */
export function previewOf(text: string): string {
  const lines = text.split("\n");
  if (lines.length <= 2) return text;
  return `${lines.slice(0, 2).join("\n").replace(/\.+$/, "")}…`;
}

function dictations(count: number): string {
  return count === 1 ? "1 dictation" : `${String(count)} dictations`;
}

/** What lowering the limit to `limit` asks, or `null` if it deletes nothing. */
export function limitQuestion(limit: number, count: number, unreadable = false): string | null {
  if (limit === 0) {
    if (count === 0) {
      return unreadable
        ? "Turn off history and delete the saved history that can't be read? This can't be undone."
        : null;
    }
    return `Turn off history and delete ${count === 1 ? "1 dictation" : `all ${dictations(count)}`}? This can't be undone.`;
  }
  const deleted = count - limit;
  if (deleted <= 0) return null;
  return `Keep the last ${String(limit)} and delete the ${deleted === 1 ? "oldest dictation" : `${String(deleted)} oldest dictations`}? This can't be undone.`;
}

/** What Clear history asks. */
export function clearQuestion(count: number, unreadable: boolean): string {
  if (count === 0) return "Delete the saved history that can't be read? This can't be undone.";
  const what = count === 1 ? "1 dictation" : `all ${dictations(count)}`;
  return unreadable
    ? `Delete ${what} and the saved history that can't be read? This can't be undone.`
    : `Delete ${what}? This can't be undone.`;
}

/** The recent dictations, read while the section shows. */
function useHistoryList() {
  const [list, setList] = useState<HistoryList | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the newest reply counts.
  const read = useRef(0);
  const refresh = useCallback(async (): Promise<HistoryList | null> => {
    read.current += 1;
    const run = read.current;
    try {
      const result = await commands.listHistory();
      if (run !== read.current) return null;
      if (result.status === "error") {
        setError(messageOf(result.error));
        return null;
      }
      setError(null);
      setList(result.data);
      return result.data;
    } catch {
      if (run === read.current) setError(BROKEN);
      return null;
    }
  }, []);
  useEffect(() => {
    // The first read; a timer, so no state changes inside the effect itself.
    const first = window.setTimeout(() => void refresh(), 0);
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") void refresh();
    }, HISTORY_POLL_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [refresh]);
  return { list, error, refresh };
}

interface Asking {
  question: string;
  confirm: string;
  run: () => void;
  /** Where the keyboard goes back to on Cancel. */
  returnTo: RefObject<HTMLElement | null>;
}

/** A small modal question: Cancel has the focus, Tab stays inside, Esc cancels. */
function Confirm({ asking, onCancel }: { asking: Asking; onCancel: () => void }) {
  const cancel = useRef<HTMLButtonElement>(null);
  const confirm = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancel.current?.focus();
  }, []);
  function onKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    } else if (event.key === "Tab") {
      event.preventDefault();
      (document.activeElement === cancel.current ? confirm : cancel).current?.focus();
    }
  }
  return (
    <>
      <div className="confirm-backdrop" />
      <div
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="history-confirm-text"
        onKeyDown={onKey}
      >
        <p id="history-confirm-text">{asking.question}</p>
        <div className="confirm-actions">
          <button ref={cancel} type="button" onClick={onCancel}>
            Cancel
          </button>
          <button
            ref={confirm}
            type="button"
            className="destructive"
            onClick={() => {
              asking.run();
            }}
          >
            {asking.confirm}
          </button>
        </div>
      </div>
    </>
  );
}

/**
 * History (U7): how many recent dictations to keep, and the list, newest
 * first. The limit applies on change through its own command, like the other
 * U6 controls; anything that deletes entries asks first. The text stays in
 * this window and goes to the clipboard through Copy, and nowhere else.
 */
export function History({
  settings,
  onStored,
}: {
  settings: Settings_Serialize;
  onStored: (settings: Settings_Serialize) => void;
}) {
  const { list, error: listError, refresh } = useHistoryList();
  const entries = list?.rows ?? null;
  const unreadable = list?.unreadable ?? false;
  const limit = useSave();
  const [asking, setAsking] = useState<Asking | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const copiedTimer = useRef<number | undefined>(undefined);
  const select = useRef<HTMLSelectElement>(null);
  const clearButton = useRef<HTMLButtonElement>(null);
  const deleteButtons = useRef(new Map<string, HTMLButtonElement>());
  // Where the keyboard goes when the question closes. Only after the render
  // that takes `inert` off this section: an inert element can't take focus.
  const focusAfterAsking = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (asking !== null) return;
    focusAfterAsking.current?.focus();
    focusAfterAsking.current = null;
  }, [asking]);

  // The times keep up, and "Copied" doesn't outlive the section.
  useEffect(() => {
    const timer = window.setInterval(() => {
      setNow(Date.now());
    }, HISTORY_POLL_MS);
    return () => {
      window.clearInterval(timer);
      window.clearTimeout(copiedTimer.current);
    };
  }, []);

  /** Runs a write that returns an error message or nothing, then reads the list. */
  async function act(write: () => Promise<string | null>): Promise<boolean> {
    let failure: string | null;
    try {
      failure = await write();
    } catch {
      failure = BROKEN;
    }
    setProblem(failure);
    await refresh();
    return failure === null;
  }

  async function applyLimit(next: number) {
    await limit.track(async () => {
      const result = await commands.setHistoryLimit(next);
      // What the app stores now, not a copy of what this pane was given.
      const reread = await commands.getSettings();
      if (reread.status === "ok") onStored(reread.data);
      await refresh();
      return result.status === "error" ? messageOf(result.error) : null;
    });
  }

  async function chooseLimit(next: number) {
    // Counted now: a dictation may have arrived since the list was read.
    const current = (await refresh()) ?? list;
    const question = limitQuestion(next, current?.rows.length ?? 0, current?.unreadable ?? false);
    if (question === null) {
      await applyLimit(next);
      return;
    }
    setAsking({
      question,
      confirm: "Delete",
      returnTo: select,
      run: () => {
        focusAfterAsking.current = select.current;
        setAsking(null);
        void applyLimit(next);
      },
    });
  }

  function askToClear() {
    const count = entries?.length ?? 0;
    setAsking({
      question: clearQuestion(count, unreadable),
      confirm: "Delete",
      returnTo: clearButton,
      run: () => {
        setAsking(null);
        void (async () => {
          await act(async () => {
            const result = await commands.clearHistory();
            return result.status === "error" ? messageOf(result.error) : null;
          });
          // The button has gone with the list; the keyboard stays in the section.
          select.current?.focus();
        })();
      },
    });
  }

  async function copy(id: string) {
    const worked = await act(async () => {
      const result = await commands.copyHistoryEntry(id);
      return result.status === "error" ? messageOf(result.error) : null;
    });
    if (!worked) return;
    setCopied(id);
    window.clearTimeout(copiedTimer.current);
    copiedTimer.current = window.setTimeout(() => {
      setCopied(null);
    }, SAVED_FOR_MS);
  }

  async function remove(id: string) {
    // Where the keyboard goes: the next row (older), else the one before.
    const rows = entries ?? [];
    const index = rows.findIndex((row) => row.id === id);
    const neighbour = rows[index + 1] ?? rows[index - 1];
    const worked = await act(async () => {
      const result = await commands.deleteHistoryEntry(id);
      return result.status === "error" ? messageOf(result.error) : null;
    });
    if (!worked) return;
    const next = neighbour === undefined ? undefined : deleteButtons.current.get(neighbour.id);
    (next ?? select.current)?.focus();
  }

  const off = settings.history_limit === 0;
  const count = entries?.length ?? 0;
  const key = keycapLabel(settings.hotkey);

  return (
    <>
      {/* While the question shows, only it can be reached, by anything. */}
      <div className="history-section" inert={asking !== null}>
        <Group>
          <Row
            label={<label htmlFor="history-limit">Keep recent dictations</label>}
            caption="Text only, kept on this Mac, never sent."
            captionId="history-limit-hint"
            below={<SaveStatus save={limit.save} />}
          >
            <select
              ref={select}
              id="history-limit"
              aria-describedby="history-limit-hint"
              value={settings.history_limit}
              onChange={(event) => {
                void chooseLimit(Number(event.target.value));
              }}
            >
              {LIMITS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Row>
        </Group>
        {listError !== null ? (
          <p role="alert" className="error">
            Couldn&apos;t read the history: {listError}
          </p>
        ) : null}
        {problem !== null ? (
          <p role="alert" className="error">
            {problem}
          </p>
        ) : null}
        {unreadable ? (
          <p role="alert" className="notice error">
            Your saved history can&apos;t be read, so it was left as it is. New dictations are kept
            only until you quit. Clear history deletes the saved file.
          </p>
        ) : null}
        {entries === null ? null : count === 0 ? (
          unreadable ? null : (
            <p className="history-empty">
              {off
                ? "History is off. New dictations aren't saved."
                : `No dictations yet. Hold ${key} and speak.`}
            </p>
          )
        ) : (
          <ul className="history-list" aria-label="Recent dictations">
            {entries.map((row) => {
              // A double that JSON can't carry comes back null; never so for a real time.
              const time = relativeTime(row.created_at ?? now, now);
              return (
                <li key={row.id} className="history-row">
                  <div className="history-meta">
                    <span className="history-time">{time}</span>
                    {row.pasted ? null : <Chip tone="attention">Not pasted</Chip>}
                  </div>
                  <p className="history-text">{previewOf(row.text)}</p>
                  <div className="history-actions">
                    <button
                      type="button"
                      // Getting the words back is why a failed paste is kept.
                      className={row.pasted ? undefined : "always-shown"}
                      aria-label={`Copy dictation from ${time}`}
                      onClick={() => void copy(row.id)}
                    >
                      {copied === row.id ? "Copied" : "Copy"}
                    </button>
                    <button
                      ref={(element) => {
                        if (element === null) deleteButtons.current.delete(row.id);
                        else deleteButtons.current.set(row.id, element);
                      }}
                      type="button"
                      aria-label={`Delete dictation from ${time}`}
                      onClick={() => void remove(row.id)}
                    >
                      Delete
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        <p role="status" className="visually-hidden">
          {copied === null ? "" : "Copied"}
        </p>
        {count === 0 && !unreadable ? null : (
          <div className="history-clear">
            <button ref={clearButton} type="button" className="destructive" onClick={askToClear}>
              Clear history…
            </button>
          </div>
        )}
      </div>
      {asking === null ? null : (
        <Confirm
          asking={asking}
          onCancel={() => {
            focusAfterAsking.current = asking.returnTo.current;
            setAsking(null);
          }}
        />
      )}
    </>
  );
}
