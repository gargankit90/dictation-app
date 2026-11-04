// The building blocks of U6's Settings panes: grouped rows (label and
// caption on the left, the control on the right), chips, a switch, and the
// brief inline "Saved" every instant-apply control shows.

import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { BROKEN } from "../keyTest";
import type { ChipTone } from "./modelRow";

/** How long "Saved" stays next to a control. */
export const SAVED_FOR_MS = 2_000;

/** A titled group of rows. */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="settings-group" aria-label={title}>
      {title === undefined ? null : <h2 className="group-title">{title}</h2>}
      <div className="group-rows">{children}</div>
    </section>
  );
}

/** One row: the label (with an id, for the control's accessible name), a caption, the control. */
export function Row({
  label,
  labelId,
  caption,
  captionId,
  children,
  below,
}: {
  label: ReactNode;
  labelId?: string;
  caption?: ReactNode;
  captionId?: string;
  children?: ReactNode;
  /** Under the row, full width: a status line, an error, a panel. */
  below?: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="row-main">
        <div className="row-text">
          <span className="row-label" id={labelId}>
            {label}
          </span>
          {caption === undefined ? null : (
            <span className="row-caption" id={captionId}>
              {caption}
            </span>
          )}
        </div>
        {children === undefined ? null : <div className="row-control">{children}</div>}
      </div>
      {below}
    </div>
  );
}

export function Chip({ tone, children }: { tone: ChipTone; children: ReactNode }) {
  return <span className={`chip ${tone}`}>{children}</span>;
}

/** A switch: a checkbox with the switch role, named by `labelledBy`. */
export function Switch({
  id,
  checked,
  onChange,
  labelledBy,
  describedBy,
  disabled = false,
}: {
  id: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  labelledBy: string;
  describedBy?: string;
  disabled?: boolean;
}) {
  return (
    <input
      id={id}
      className="switch"
      type="checkbox"
      role="switch"
      checked={checked}
      disabled={disabled}
      aria-checked={checked}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      onChange={(event) => {
        onChange(event.target.checked);
      }}
    />
  );
}

export type SaveState =
  | { state: "idle" }
  | { state: "saving" }
  | { state: "saved" }
  | { state: "error"; message: string };

/**
 * A control's save: `run` awaits a write that returns an error message (or
 * `null`), shows "Saved" briefly on success and the message on failure.
 * Only the newest write's outcome shows.
 */
export function useSave() {
  const [save, setSave] = useState<SaveState>({ state: "idle" });
  const run = useRef(0);
  const timer = useRef<number | undefined>(undefined);
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
    },
    [],
  );
  const track = useCallback(async (write: () => Promise<string | null>): Promise<boolean> => {
    run.current += 1;
    const mine = run.current;
    window.clearTimeout(timer.current);
    setSave({ state: "saving" });
    let error: string | null;
    try {
      error = await write();
    } catch {
      error = BROKEN;
    }
    if (mine !== run.current) return error === null;
    if (error === null) {
      setSave({ state: "saved" });
      timer.current = window.setTimeout(() => {
        setSave({ state: "idle" });
      }, SAVED_FOR_MS);
      return true;
    }
    setSave({ state: "error", message: error });
    return false;
  }, []);
  const clear = useCallback(() => {
    run.current += 1;
    window.clearTimeout(timer.current);
    setSave({ state: "idle" });
  }, []);
  return { save, track, clear };
}

/** "Saved" or the error, next to the control that was changed. */
export function SaveStatus({ save, id }: { save: SaveState; id?: string }) {
  if (save.state === "saved") {
    return (
      <span id={id} role="status" className="saved-note">
        Saved
      </span>
    );
  }
  if (save.state === "error") {
    return (
      <p id={id} role="alert" className="error row-error">
        {save.message}
      </p>
    );
  }
  return null;
}
