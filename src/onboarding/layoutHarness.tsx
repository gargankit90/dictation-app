// A test page, not part of the app (no page of the app imports it): the real
// onboarding at its minimum content size (680×520 less the title bar) with
// every text made pseudo-long, on a fake backend, measured in real WebKit. Built by
// vite.layout.config.ts and opened by e2e_onboarding_layout, which reads the
// verdict line. U8 criterion 1: jsdom can't measure layout.

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

const WIDTH = 680;
/** The window's minimum height less its title bar. */
const HEIGHT = 492;
/** Devanagari with no break, then a third again of ordinary words. */
const PSEUDO = "हिन्दीकेलंबेशब्दोंकीजाँचहिन्दीकेलंबे";
const SETTLE_MS = 1800;

// Which step: a page per step, since macOS's `open` drops a file URL's query.
const view = document.body.dataset.step ?? "welcome";
const steps: Record<string, Step> = {
  welcome: "welcome",
  speech: "speech",
  "speech-key": "speech",
  permissions: "permissions",
  try: "try",
  ready: "ready",
};

const model: LocalModelInfo_Serialize = {
  id: "parakeet",
  name: "Parakeet Unified 0.6B",
  languages: "English only",
  language_codes: ["en"],
  size_bytes: 477_000_000,
  downloaded_bytes: 212_000_000,
  state: "downloading",
  resumed_from_bytes: 100_000_000,
  restarted: false,
  error: null,
  default: true,
};

const settings: Settings_Serialize = {
  version: 2,
  provider: "groq",
  model: "whisper-large-v3-turbo",
  language: "hi",
  hotkey: "RightAlt",
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
  content.scrollTop = content.scrollHeight;
  const last = content.lastElementChild?.getBoundingClientRect();
  if (last !== undefined && last.bottom > area.bottom + 1) {
    found.push("the end of the content can't be scrolled to");
  }
  content.scrollTop = 0;

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

const page = document.getElementById("harness");
const verdict = document.getElementById("verdict");
if (page === null || verdict === null) throw new Error("the harness page is incomplete");
page.style.width = `${String(WIDTH)}px`;
page.style.height = `${String(HEIGHT)}px`;

createRoot(page).render(<Onboarding onFinished={() => undefined} />);
makePseudoLong(page);

/** Measures once the step is on screen and the text has settled. */
function measureWhenReady() {
  if (page === null || verdict === null) return;
  if (page.querySelector(".ob-footer") === null) {
    window.setTimeout(measureWhenReady, 200);
    return;
  }
  window.setTimeout(() => {
    const found = [...new Set(problems(page))];
    verdict.textContent = found.length === 0 ? `LAYOUT ${view} OK` : `LAYOUT ${view} FAILED`;
    // A line each: the accessibility dump cuts long values.
    for (const problem of found) {
      const line = document.createElement("p");
      line.textContent = `LAYOUT ${view} FAIL: ${problem}`;
      verdict.after(line);
    }
  }, SETTLE_MS);
}

measureWhenReady();
