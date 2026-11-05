// The frame every step sits in (U8): the scrolling content, the download
// strip, and the fixed footer with Back, the reason a disabled primary is
// disabled, and the primary action.

import { useEffect, useRef } from "react";
import type { ReactNode } from "react";

export interface Primary {
  label: string;
  onClick: () => void;
  /**
   * Why it can't be pressed yet, or `null` if it can. There is no other way
   * to disable it, so a disabled primary always says why.
   */
  blockedBecause: string | null;
}

export interface Secondary {
  label: string;
  onClick: () => void;
}

/** What the shell gives each step for its frame. */
export interface FrameParts {
  /** Goes to the step before; `null` on the first step. */
  back: (() => void) | null;
  /** The download strip, if there is one. */
  strip: ReactNode;
  /** Why the last step change didn't work. */
  error: string | null;
  /** A step change is running. */
  busy: boolean;
  /**
   * Why nothing may leave this step yet (the sharing switch isn't saved), or
   * `null`. Disables what leaves; the shell also refuses the exits themselves.
   */
  exitBlock: string | null;
}

interface Props {
  parts: FrameParts;
  title: string;
  /** Welcome's larger title. */
  large?: boolean;
  primary: Primary;
  secondary?: Secondary;
  children: ReactNode;
}

export function StepFrame({ parts, title, large = false, primary, secondary, children }: Props) {
  const content = useRef<HTMLDivElement>(null);
  // A new step starts at its top.
  useEffect(() => {
    if (content.current !== null) content.current.scrollTop = 0;
  }, [title]);
  const blocked = parts.busy ? "One moment…" : (parts.exitBlock ?? primary.blockedBecause);
  const stuck = parts.busy || parts.exitBlock !== null;
  return (
    <div className="ob-main">
      <div className="ob-content" ref={content}>
        <h1 className={large ? "ob-title-large" : undefined}>{title}</h1>
        {children}
        {parts.error !== null && (
          <p role="alert" className="notice error">
            {parts.error}
          </p>
        )}
      </div>
      {parts.strip}
      <footer className="ob-footer">
        {parts.back === null ? (
          <span className="ob-footer-space" />
        ) : (
          <button type="button" className="ob-back" disabled={stuck} onClick={parts.back}>
            Back
          </button>
        )}
        <p className="ob-reason" id="ob-reason" role="status">
          {blocked}
        </p>
        {secondary !== undefined && (
          <button
            type="button"
            className="ob-secondary"
            disabled={stuck}
            onClick={secondary.onClick}
          >
            {secondary.label}
          </button>
        )}
        <button
          type="button"
          className="primary"
          disabled={blocked !== null}
          aria-describedby={blocked === null ? undefined : "ob-reason"}
          onClick={primary.onClick}
        >
          {primary.label}
        </button>
      </footer>
    </div>
  );
}
