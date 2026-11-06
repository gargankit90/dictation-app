// The logic behind the dictation-key recorder: turns key events into the
// hotkey spec text (`RightCmd`, `Ctrl+Shift+Space`; see
// `crates/core/src/hotkey_spec.rs`) and spec text into a keycap label. Pure,
// so it is tested without a window. The app still validates on save.

export const DEFAULT_HOTKEY = "RightAlt";

type Mod = "Ctrl" | "Alt" | "Shift" | "Cmd";
type Side = "Left" | "Right";

const MODIFIER_ORDER: readonly Mod[] = ["Ctrl", "Alt", "Shift", "Cmd"];

/** `KeyboardEvent.code` of each modifier key. */
const MODIFIER_CODES: Record<string, { mod: Mod; side: Side }> = {
  ControlLeft: { mod: "Ctrl", side: "Left" },
  ControlRight: { mod: "Ctrl", side: "Right" },
  AltLeft: { mod: "Alt", side: "Left" },
  AltRight: { mod: "Alt", side: "Right" },
  ShiftLeft: { mod: "Shift", side: "Left" },
  ShiftRight: { mod: "Shift", side: "Right" },
  MetaLeft: { mod: "Cmd", side: "Left" },
  MetaRight: { mod: "Cmd", side: "Right" },
};

const NAMED_KEYS: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** Keys that type text, so they need Ctrl, Alt or Cmd to be a hotkey. */
const TEXT_KEYS = /^(?:[A-Z]|[0-9]|Space|Enter|Tab|Backspace)$/u;

/** The spec name of a non-modifier key, or `null` if no hotkey can be it. */
function keyName(code: string): string | null {
  const named = NAMED_KEYS[code];
  if (named !== undefined) return named;
  const letter = /^Key([A-Z])$/u.exec(code);
  if (letter?.[1] !== undefined) return letter[1];
  const digit = /^Digit([0-9])$/u.exec(code);
  if (digit?.[1] !== undefined) return digit[1];
  const fn = /^F([1-9]|1[0-9]|20)$/u.exec(code);
  if (fn?.[1] !== undefined) return `F${fn[1]}`;
  return null;
}

/** What the recorder has seen since it started listening. */
export interface RecorderState {
  /** Modifier codes held right now. */
  down: readonly string[];
  /** Every modifier code pressed since the last result, in order. */
  seen: readonly string[];
  /**
   * A key that isn't a modifier was pressed and refused. Letting go of the
   * modifiers then is not a choice of a modifier-only key.
   */
  spoiled: boolean;
}

export const INITIAL_RECORDER: RecorderState = { down: [], seen: [], spoiled: false };

export type RecorderOutcome =
  { kind: "recorded"; spec: string } | { kind: "refused"; reason: string } | { kind: "cancelled" };

export interface RecorderStep {
  state: RecorderState;
  /** Set when this event ended the recording attempt. */
  outcome?: RecorderOutcome;
}

/** A key event: its physical key and, when known, the modifiers held. */
export interface RecorderEvent {
  type: "down" | "up";
  code: string;
  /** `KeyboardEvent`'s modifier flags, which say what is held right now. */
  flags?: { ctrl: boolean; alt: boolean; shift: boolean; meta: boolean };
}

const REFUSE_UNKNOWN = "That key can't be a dictation key. Try another, such as a modifier key.";

const MODIFIER_NAMES: Record<Mod, string> = {
  Ctrl: "Ctrl",
  Alt: "Option",
  Shift: "Shift",
  Cmd: "Command",
};

/** A refusal; the keys physically held stay held, so the next try counts them. */
function refused(state: RecorderState, reason: string): RecorderStep {
  return { state: { ...state, spoiled: true }, outcome: { kind: "refused", reason } };
}

/** The modifier keys held according to `flags`, if the event has them. */
function heldModifiers(state: RecorderState, flags: RecorderEvent["flags"]): Set<Mod> {
  if (flags === undefined) {
    return new Set(state.down.flatMap((c) => MODIFIER_CODES[c]?.mod ?? []));
  }
  const held = new Set<Mod>();
  if (flags.ctrl) held.add("Ctrl");
  if (flags.alt) held.add("Alt");
  if (flags.shift) held.add("Shift");
  if (flags.meta) held.add("Cmd");
  return held;
}

/**
 * One key event while recording. A non-modifier key ends it at once, with
 * the modifiers held then (either side, as `Ctrl+Shift+Space`), including
 * ones held before recording began. Modifiers pressed and all released with
 * no other key make a modifier-only key that keeps its sides (`RightCmd`).
 * Esc cancels. A refused key leaves the held modifiers as they are, and
 * letting go of them afterwards records nothing.
 */
export function recorderStep(before: RecorderState, event: RecorderEvent): RecorderStep {
  const { code } = event;
  const modifier = MODIFIER_CODES[code];
  // A modifier the flags say is up was let go where we couldn't see it.
  const flagged = event.flags === undefined ? null : heldModifiers(before, event.flags);
  const state: RecorderState =
    flagged === null
      ? before
      : {
          ...before,
          down: before.down.filter((c) => {
            // The key being let go is handled below, whatever the flags say.
            if (event.type === "up" && c === code) return true;
            const mod = MODIFIER_CODES[c]?.mod;
            return mod !== undefined && flagged.has(mod);
          }),
        };
  if (event.type === "down") {
    if (code === "Escape") return { state: INITIAL_RECORDER, outcome: { kind: "cancelled" } };
    if (modifier !== undefined) {
      return {
        state: {
          ...state,
          down: state.down.includes(code) ? state.down : [...state.down, code],
          seen: state.seen.includes(code) ? state.seen : [...state.seen, code],
        },
      };
    }
    const name = keyName(code);
    if (name === null) return refused(state, REFUSE_UNKNOWN);
    const held = heldModifiers(state, event.flags);
    if (TEXT_KEYS.test(name) && !held.has("Ctrl") && !held.has("Alt") && !held.has("Cmd")) {
      return refused(
        state,
        `A plain ${name} would start dictation whenever you type it. Hold Ctrl, Option or Command with it, or use a modifier key alone.`,
      );
    }
    const parts = [...MODIFIER_ORDER.filter((m) => held.has(m)), name];
    return { state: INITIAL_RECORDER, outcome: { kind: "recorded", spec: parts.join("+") } };
  }
  if (modifier === undefined || !state.down.includes(code)) return { state };
  const down = state.down.filter((c) => c !== code);
  if (down.length > 0) return { state: { ...state, down } };
  // Every modifier is up. After a refused key that is only the end of the
  // attempt; otherwise no other key came, so the modifiers were the key.
  if (state.spoiled) return { state: INITIAL_RECORDER };
  const sided = new Map<Mod, Side>();
  for (const seen of state.seen) {
    const found = MODIFIER_CODES[seen];
    if (found === undefined) continue;
    if (sided.has(found.mod) && sided.get(found.mod) !== found.side) {
      return {
        state: INITIAL_RECORDER,
        outcome: {
          kind: "refused",
          reason: `Use one side of ${MODIFIER_NAMES[found.mod]}, not both.`,
        },
      };
    }
    sided.set(found.mod, found.side);
  }
  const parts = MODIFIER_ORDER.flatMap((m) => {
    const side = sided.get(m);
    return side === undefined ? [] : [`${side}${m}`];
  });
  return { state: INITIAL_RECORDER, outcome: { kind: "recorded", spec: parts.join("+") } };
}

const SYMBOLS: Record<Mod, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Cmd: "⌘" };

const ALIASES: Record<string, Mod> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "Alt",
  option: "Alt",
  opt: "Alt",
  shift: "Shift",
  cmd: "Cmd",
  command: "Cmd",
  meta: "Cmd",
  super: "Cmd",
  win: "Cmd",
};

/**
 * The keycap text for a stored hotkey: `RightAlt` → `Right ⌥`,
 * `Ctrl+Shift+Space` → `⌃⇧Space`. Anything unrecognised is shown as written.
 */
export function keycapLabel(spec: string): string {
  const parts = spec
    .split("+")
    .map((p) => p.trim())
    .filter((p) => p !== "");
  const labels = parts.map((part) => {
    const lower = part.toLowerCase();
    const side = lower.startsWith("left") ? "Left" : lower.startsWith("right") ? "Right" : null;
    const mod = ALIASES[side === null ? lower : lower.slice(side.length)];
    if (mod === undefined) return { text: part, side: null };
    return { text: SYMBOLS[mod], side };
  });
  if (labels.some((l) => l.side !== null)) {
    return labels.map((l) => (l.side === null ? l.text : `${l.side} ${l.text}`)).join(" ");
  }
  return labels.map((l) => l.text).join("");
}

/** Each modifier's name in words, on the Mac. */
const LONG_NAMES: Record<Mod, string> = {
  Ctrl: "Control",
  Alt: "Option",
  Shift: "Shift",
  Cmd: "Command",
};

/** A spec's parts in words, with or without the modifier's symbol. */
function longParts(spec: string, symbols: boolean): string[] {
  return spec
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part !== "")
    .map((part) => {
      const lower = part.toLowerCase();
      const side = lower.startsWith("left") ? "Left" : lower.startsWith("right") ? "Right" : null;
      const mod = ALIASES[side === null ? lower : lower.slice(side.length)];
      if (mod === undefined) return part;
      const name = side === null ? LONG_NAMES[mod] : `${side} ${LONG_NAMES[mod]}`;
      return symbols ? `${name} (${SYMBOLS[mod]})` : name;
    });
}

/**
 * The key in full, for where there is room (U11): `RightAlt` → `Right Option
 * (⌥)`, `Ctrl+Shift+Space` → `Control (⌃) + Shift (⇧) + Space`. A key that isn't
 * a modifier is shown as written, as on the keycap.
 */
export function keyLongLabel(spec: string): string {
  return longParts(spec, true).join(" + ");
}

/** The same in words only, for a sentence read aloud: `Right Option`. */
export function keySpokenName(spec: string): string {
  return longParts(spec, false).join(" + ");
}
