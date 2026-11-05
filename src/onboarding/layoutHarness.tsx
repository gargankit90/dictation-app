// A test page, not part of the app (no page of the app imports it): the real
// onboarding at its minimum content size (680×520 less the title bar) with
// every text made pseudo-long, on a fake backend, measured in real WebKit. Built by
// vite.layout.config.ts and opened by e2e_onboarding_layout, which reads the
// verdict line. U8 criterion 1: jsdom can't measure layout. U9 adds what also
// takes a real browser: the selected language button in each state, the failed
// paste panel and the text box's focus on Try it, and the language search box.

import { mockIPC } from "@tauri-apps/api/mocks";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import type {
  DictationProgressView,
  LocalModelInfo_Serialize,
  OnboardingStatus,
  Settings_Serialize,
  Step,
} from "../bindings";
import { Onboarding } from "../Onboarding";
import { Settings } from "../Settings";
import settingsCss from "../settings.css?inline";
import tokensCss from "../styles/tokens.css?inline";

/** The window's minimum size, less its title bar. */
const MIN = { width: 680, height: 492 };
/** The window's default size, less its title bar. */
const DEFAULT = { width: 740, height: 532 };
/** Devanagari with no break, then a third again of ordinary words. */
const PSEUDO = "हिन्दीकेलंबेशब्दोंकीजाँचहिन्दीकेलंबे";
const SETTLE_MS = 1800;

// Which step: a page per step, since macOS's `open` drops a file URL's query.
const view = document.body.dataset.step ?? "welcome";
const isMore = view === "speech-more" || view === "speech-more-plain";
const steps: Record<string, Step> = {
  welcome: "welcome",
  speech: "speech",
  "speech-key": "speech",
  permissions: "permissions",
  "speech-more": "speech",
  "speech-more-plain": "speech",
  try: "try",
  "try-plain": "try",
  "try-default": "try",
  ready: "ready",
};
const size = view === "try-default" ? DEFAULT : MIN;
const isTry = steps[view] === "try";
const isLanguage = isMore || view === "settings-language";

/** The owner's Try it, as it is, not pseudo-long. */
const plain =
  view === "try-plain" ||
  view === "try-default" ||
  view === "speech-more-plain" ||
  view === "settings-language";

const model: LocalModelInfo_Serialize = {
  id: "parakeet",
  name: "Parakeet Unified 0.6B",
  languages: "English only",
  language_codes: ["en"],
  size_bytes: 477_000_000,
  downloaded_bytes: plain ? 477_000_000 : 212_000_000,
  // The owner's Try it has its model: practice is open and the box can be focused.
  state: plain ? "ready" : "downloading",
  resumed_from_bytes: plain ? null : 100_000_000,
  restarted: false,
  error: null,
  default: true,
};

const settings: Settings_Serialize = {
  version: 2,
  provider: "groq",
  model: "whisper-large-v3-turbo",
  language: isMore ? "fr" : "hi",
  // The owner's state on Try it: a recorded two-key shortcut.
  hotkey: isTry ? "RightAlt+RightCmd" : "RightAlt",
  launch_at_login: false,
  transcription: view === "speech-key" ? "cloud" : "local",
  local_model: "parakeet",
  share_usage_events: true,
  history_limit: 25,
};

const status: OnboardingStatus = {
  view: "onboarding",
  step: steps[view] ?? "welcome",
  microphone: view === "permissions" ? "denied" : "granted",
  accessibility: view === "permissions" ? "not_determined" : "granted",
  needs_relaunch: false,
  try_outcome: "not_tried",
  download_paused: false,
  speech_choice: view === "speech-key" ? "own_key" : null,
};

const quiet: DictationProgressView = {
  started: 0,
  finished: 0,
  completed: 0,
  errors: 0,
  stages: [],
  error: null,
  failure: null,
  heard: false,
};
const missed: DictationProgressView = {
  ...quiet,
  started: 1,
  finished: 1,
  errors: 1,
  stages: ["recorded", "transcribed"],
  error: "Vocet couldn't paste the text.",
  failure: "paste",
  heard: true,
};

let progressCalls = 0;

mockIPC((command) => {
  switch (command) {
    case "get_onboarding":
    case "set_onboarding_step":
      return status;
    case "get_settings":
      return settings;
    case "get_launch_at_login":
      return false;
    case "get_hotkey_notice":
      return view === "settings-language"
        ? "The hotkey Right ⌥ is saved, but Vocet can't listen to the keyboard yet: allow it in System Settings → Privacy & Security → Accessibility, and Right ⌥ starts working by itself."
        : null;
    case "get_app_info":
      return { version: "0.1.0", last_update_check_ms: null };
    case "list_local_models":
      return [{ ...model }];
    case "get_engine_status":
      return { state: "ready", message: null };
    case "has_api_key":
      return false;
    case "dictation_progress":
      progressCalls += 1;
      return progressCalls === 1 ? quiet : missed;
    case "practice_words":
      return `${PSEUDO} ${PSEUDO} ${PSEUDO}`;
    case "read_microphone_test":
      return { level: -20, device: `${PSEUDO} Microphone` };
    default:
      return null;
  }
});

/** Makes every text under `root` long, including what React writes later. */
function makePseudoLong(root: HTMLElement) {
  const expand = (node: Text) => {
    const text = node.nodeValue ?? "";
    if (text.trim().length < 2 || text.includes(PSEUDO)) return;
    node.nodeValue = `${text} ${text.slice(0, Math.ceil(text.length / 3))} ${PSEUDO}`;
  };
  const sweep = () => {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      expand(node as Text);
    }
  };
  new MutationObserver(sweep).observe(root, {
    childList: true,
    characterData: true,
    subtree: true,
  });
  sweep();
}

function overlap(a: DOMRect, b: DOMRect): boolean {
  return (
    a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1
  );
}

/** Everything that must hold for the step to be usable; empty if it all does. */
function problems(root: HTMLElement): string[] {
  const found: string[] = [];
  const bounds = root.getBoundingClientRect();
  const inside = (name: string, rect: DOMRect, area = bounds) => {
    if (
      rect.left < area.left - 1 ||
      rect.right > area.right + 1 ||
      rect.top < area.top - 1 ||
      rect.bottom > area.bottom + 1
    ) {
      found.push(`${name} is outside its area`);
    }
  };
  const unclipped = (name: string, element: Element) => {
    if (
      element.scrollWidth > element.clientWidth + 1 ||
      element.scrollHeight > element.clientHeight + 1
    ) {
      found.push(`${name} is clipped`);
    }
  };

  const content = root.querySelector<HTMLElement>(".ob-content");
  const footer = root.querySelector<HTMLElement>(".ob-footer");
  const primary = root.querySelector<HTMLElement>(".ob-footer .primary");
  if (content === null || footer === null || primary === null) return ["the frame is missing"];

  const button = primary.getBoundingClientRect();
  inside("the primary action", button);
  unclipped("the primary action", primary);
  if (button.height < 44) found.push("the primary action is under 44 px");
  const hit = document.elementFromPoint(
    button.left + button.width / 2,
    button.top + button.height / 2,
  );
  if (hit === null || !primary.contains(hit)) found.push("the primary action is covered");

  const parts = [...footer.children].filter((part) => part.getBoundingClientRect().width > 0);
  parts.forEach((part, index) => {
    inside(`footer part ${String(index)}`, part.getBoundingClientRect());
    unclipped(`footer part ${String(index)}`, part);
    for (const other of parts.slice(index + 1)) {
      if (overlap(part.getBoundingClientRect(), other.getBoundingClientRect())) {
        found.push(`footer parts ${String(index)} and ${String(parts.indexOf(other))} overlap`);
      }
    }
  });
  if (primary.hasAttribute("disabled")) {
    const reason = root.querySelector<HTMLElement>(".ob-reason");
    if (!reason?.textContent.trim()) found.push("a disabled primary has no reason");
  }

  const area = content.getBoundingClientRect();
  if (area.height < 120) found.push(`the content is only ${String(area.height)} px tall`);
  if (content.scrollWidth > content.clientWidth + 1) found.push("the content overflows sideways");
  // The fix panel scrolls itself into view; taller than the area, it fills it.
  const fix = root.querySelector(".ob-fix-area")?.getBoundingClientRect();
  if (
    fix !== undefined &&
    (fix.bottom > area.bottom + 1 || fix.top < area.top - 1) &&
    !(fix.top <= area.top + 1 && fix.bottom >= area.bottom - 1)
  ) {
    found.push("the fix panel isn't scrolled into view");
  }
  // Put back after: the run's picture shows what the user would be looking at.
  const scrolled = content.scrollTop;
  content.scrollTop = content.scrollHeight;
  const last = content.lastElementChild?.getBoundingClientRect();
  if (last !== undefined && last.bottom > area.bottom + 1) {
    found.push("the end of the content can't be scrolled to");
  }
  content.scrollTop = scrolled;

  const rail = root.querySelector<HTMLElement>(".ob-rail");
  if (rail === null) found.push("the rail is missing");
  else if (rail.scrollWidth > rail.clientWidth + 1) found.push("the rail overflows sideways");

  for (const scope of [".ob-content", ".ob-rail", ".ob-strip"]) {
    const box = root.querySelector(scope);
    if (box === null) continue;
    const limit = box.getBoundingClientRect();
    for (const element of box.querySelectorAll<HTMLElement>("*")) {
      if (element.closest(".ob-sr") !== null || element.closest("details:not([open])") !== null) {
        continue;
      }
      const rect = element.getBoundingClientRect();
      if (rect.width > 0 && rect.right > limit.right + 1) {
        found.push(`${element.tagName.toLowerCase()} in ${scope} sticks out`);
        break;
      }
    }
  }

  const strip = root.querySelector(".ob-strip");
  if (strip !== null) inside("the strip", strip.getBoundingClientRect());
  return found;
}

function settle(ms = 250): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** What a CSS colour value computes to here, such as `var(--vc-accent)`. */
function colour(value: string): string {
  const probe = document.createElement("span");
  probe.style.color = value;
  document.body.append(probe);
  const computed = getComputedStyle(probe).color;
  probe.remove();
  return computed;
}

function luminance(rgb: string): number {
  const [r = 0, g = 0, b = 0] = (rgb.match(/[\d.]+/g) ?? []).map(Number).map((channel) => {
    const c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

/** Whether the middle of `element` can be seen and clicked. */
function reachable(element: Element): boolean {
  const rect = element.getBoundingClientRect();
  const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
  return hit !== null && (element === hit || element.contains(hit));
}

/**
 * The cascade decides how a state looks and only a browser runs it, but a script
 * can't hover, and `:focus` needs the window to be the active one. So `read` runs
 * with the app's own CSS with each state as a class: a class weighs what the
 * pseudo-class does, so what wins is what would win.
 */
async function withStatesAsClasses<T>(read: () => T): Promise<T> {
  const forced = `${tokensCss}\n${settingsCss}`
    .replace(/:hover/g, ".is-hover")
    .replace(/:active/g, ".is-active")
    .replace(/:focus-visible/g, ".is-focus-visible")
    .replace(/:focus/g, ".is-focus");
  const style = document.createElement("style");
  style.textContent = `${forced}\n* { transition: none !important; }`;
  const link = document.querySelector<HTMLLinkElement>('link[rel="stylesheet"]');
  if (link !== null) link.disabled = true;
  document.head.append(style);
  await settle();
  const result = read();
  style.remove();
  if (link !== null) link.disabled = false;
  await settle();
  return result;
}

/**
 * U9 criterion 4. "After a click" is hovered and focused; a window losing focus
 * changes no style, as the CSS has no rule for it.
 */
async function chipProblems(root: HTMLElement): Promise<string[]> {
  const chip = root.querySelector('.ob-chip[aria-checked="true"]');
  if (chip === null) return ["no language button is selected"];
  return withStatesAsClasses(() => {
    const filled = [colour("var(--vc-accent)"), colour("var(--vc-accent-hover)")];
    const found: string[] = [];
    const states: string[][] = [
      [],
      ["is-hover"],
      ["is-hover", "is-active"],
      ["is-focus", "is-focus-visible"],
      ["is-hover", "is-focus", "is-focus-visible"],
      ["is-hover", "is-active", "is-focus", "is-focus-visible"],
    ];
    for (const state of states) {
      chip.classList.add(...state);
      const computed = getComputedStyle(chip);
      const name = state.join("+") || "resting";
      if (!filled.includes(computed.backgroundColor)) {
        found.push(`the selected chip loses its fill when ${name}`);
      }
      if (contrast(computed.color, computed.backgroundColor) < 4.5) {
        found.push(`the selected chip's label is unreadable when ${name}`);
      }
      chip.classList.remove(...state);
    }
    return found;
  });
}

/** U9 criteria 5 and 6: the panel, and the box's focus, in the owner's state. */
async function tryProblems(root: HTMLElement): Promise<string[]> {
  const found: string[] = [];
  const content = root.querySelector<HTMLElement>(".ob-content");
  const box = root.querySelector<HTMLTextAreaElement>("#try-field");
  if (content === null || box === null) return ["Try it isn't on screen"];
  const panelInView = (when: string) => {
    const area = content.getBoundingClientRect();
    const panel = root.querySelector(".ob-fix-area");
    const copy = panel === null ? null : panel.querySelector("button");
    if (panel === null || copy === null) {
      found.push(`the failed-paste panel is missing ${when}`);
      return;
    }
    const rect = panel.getBoundingClientRect();
    // Pseudo-long text can make it taller than the area: then only its button
    // has to be there.
    const fits = rect.height <= area.height;
    if (fits && (rect.top < area.top - 1 || rect.bottom > area.bottom + 1)) {
      found.push(`the failed-paste panel isn't all in view ${when}`);
    }
    if (!reachable(copy)) found.push(`the panel's button can't be clicked ${when}`);
  };
  panelInView("at first");
  // Only `try` has the model still on its way, which makes practice wait.
  if (box.disabled) return found;
  if (document.activeElement !== box) found.push("the box has no focus on arriving");

  const record = root.querySelector<HTMLElement>(".hotkey-recorder button");
  record?.click();
  await settle();
  // A real click on the button takes the focus from the box; a script's doesn't.
  box.blur();
  window.dispatchEvent(new Event("focus"));
  await settle();
  if (document.activeElement === box) found.push("the box takes focus during a recording");
  root.querySelector<HTMLElement>(".hotkey-recorder button")?.click();
  await settle();
  if (document.activeElement !== box) found.push("the box has no focus after a cancel");
  if (box.value !== "") found.push("the box isn't empty after a recording");
  panelInView("after a cancel");

  box.blur();
  content.scrollTop = 0;
  window.dispatchEvent(new Event("focus"));
  await settle();
  if (document.activeElement !== box) found.push("the box has no focus when the window returns");
  panelInView("when the window returns");
  return found;
}

/** U9 criterion 7: the search box, in setup and in Settings, at the minimum. */
async function languageProblems(root: HTMLElement): Promise<string[]> {
  const found: string[] = [];
  const field = root.querySelector<HTMLInputElement>('.language-picker input[role="combobox"]');
  const scroller = field === null ? null : field.closest(".ob-content, .settings-pane");
  if (field === null || scroller === null) return ["the language search box isn't on screen"];
  const style = getComputedStyle(field);
  const chevron = root.querySelector(".language-picker .language-chevron");
  if (chevron === null || chevron.getBoundingClientRect().width === 0) {
    found.push("the closed search box has no chevron");
  }
  if (parseFloat(style.borderTopLeftRadius) < 1) found.push("the search box isn't rounded");
  if (style.borderTopColor !== colour("var(--vc-line-strong)")) {
    found.push("the search box's border isn't the design's");
  }
  const ring = await withStatesAsClasses(() => {
    field.classList.add("is-focus", "is-focus-visible");
    const computed = getComputedStyle(field);
    const said = [
      computed.outlineStyle,
      computed.outlineWidth,
      computed.outlineOffset,
      computed.outlineColor,
    ].join(" ");
    field.classList.remove("is-focus", "is-focus-visible");
    return said;
  });
  if (ring !== `solid 3px 2px ${colour("var(--vc-accent)")}`) {
    found.push(`the search box hasn't the standard focus ring: ${ring}`);
  }
  field.focus();
  await settle(400);
  const list = root.querySelector(".language-list");
  const first = list === null ? null : list.querySelector('[role="option"]');
  if (list === null || first === null) return [...found, "focusing the search box opens no list"];
  const area = scroller.getBoundingClientRect();
  const box = field.getBoundingClientRect();
  const rows = list.getBoundingClientRect();
  // The ring is 5 px out: a row under it is cut off.
  if (rows.top < box.bottom + 5) found.push("the list's top sits under the field's ring");
  if (box.top < area.top - 1) found.push("the field is scrolled out of its area");
  if (rows.bottom > area.bottom + 1) found.push("the list is cut off by its area");
  if (rows.right > area.right + 1) found.push("the list sticks out sideways");
  // The row it opens at: the selected language, which may be far down the list.
  const opened = field.getAttribute("aria-activedescendant");
  const start = opened === null ? first : document.getElementById(opened);
  if (start === null || !reachable(start)) found.push("the list's active row can't be clicked");

  const key = (name: string) =>
    field.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
  const before = field.getAttribute("aria-activedescendant");
  key("ArrowDown");
  await settle();
  const after = field.getAttribute("aria-activedescendant");
  if (after === before && (list.children.length > 1 || after === null)) {
    found.push("the arrow keys don't move in the list");
  }
  const active = after === null ? null : document.getElementById(after);
  if (active !== null) {
    const row = active.getBoundingClientRect();
    if (row.top < rows.top - 1 || row.bottom > rows.bottom + 1) {
      found.push("the active row isn't in view");
    }
  }
  key("Escape");
  await settle();
  if (field.getAttribute("aria-expanded") !== "false") found.push("Esc doesn't close the list");
  // For the pictures: setup's shows the list open, Settings' the field closed.
  if (isMore) {
    key("ArrowDown");
    await settle();
  }
  return found;
}

/** Everything U9 measures for this page, on top of the frame's own checks. */
async function viewProblems(root: HTMLElement): Promise<string[]> {
  const found: string[] = [];
  if (view === "speech" || isMore) found.push(...(await chipProblems(root)));
  if (isTry) found.push(...(await tryProblems(root)));
  if (isLanguage) found.push(...(await languageProblems(root)));
  return found;
}

const page = document.getElementById("harness");
const verdict = document.getElementById("verdict");
if (page === null || verdict === null) throw new Error("the harness page is incomplete");
page.style.width = `${String(size.width)}px`;
page.style.height = `${String(size.height)}px`;

createRoot(page).render(
  view === "settings-language" ? <Settings /> : <Onboarding onFinished={() => undefined} />,
);
if (!plain) makePseudoLong(page);

/** Settings, at the section with the search box. */
function openDictation(root: HTMLElement): boolean {
  if (root.querySelector("#language") !== null) return true;
  root.querySelector<HTMLElement>("#tab-dictation")?.click();
  return false;
}

async function measure(root: HTMLElement, out: HTMLElement) {
  const own = await viewProblems(root);
  const found = [...new Set([...own, ...(view === "settings-language" ? [] : problems(root))])];
  out.textContent = found.length === 0 ? `LAYOUT ${view} OK` : `LAYOUT ${view} FAILED`;
  // A line each: the accessibility dump cuts long values.
  for (const problem of found) {
    const line = document.createElement("p");
    line.textContent = `LAYOUT ${view} FAIL: ${problem}`;
    out.after(line);
  }
}

/** Measures once the step is on screen and the text has settled. */
function measureWhenReady() {
  if (page === null || verdict === null) return;
  const ready =
    view === "settings-language" ? openDictation(page) : page.querySelector(".ob-footer") !== null;
  if (!ready) {
    window.setTimeout(measureWhenReady, 200);
    return;
  }
  window.setTimeout(() => {
    void measure(page, verdict);
  }, SETTLE_MS);
}

measureWhenReady();
