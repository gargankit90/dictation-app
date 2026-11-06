import { afterEach } from "vitest";
import { resetSharingQueue } from "./settings/sharingQueue";

// The sharing queue is one per window; each test is a new window.
afterEach(() => {
  resetSharingQueue();
});

// jsdom doesn't implement scrollIntoView; the language picker calls it.
if (typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = () => undefined;
}

// jsdom has no matchMedia; Welcome's demo asks about Reduce Motion. No media
// query matches unless a test says so with `setReducedMotion`.
const motion = { reduced: false, listeners: new Set<() => void>() };

/** What the system says about Reduce Motion, for this test. */
export function setReducedMotion(reduced: boolean) {
  motion.reduced = reduced;
  for (const listener of motion.listeners) listener();
}

window.matchMedia = (query: string) =>
  ({
    get matches() {
      return query.includes("prefers-reduced-motion") && motion.reduced;
    },
    media: query,
    addEventListener: (_type: string, listener: () => void) => {
      motion.listeners.add(listener);
    },
    removeEventListener: (_type: string, listener: () => void) => {
      motion.listeners.delete(listener);
    },
  }) as unknown as MediaQueryList;

afterEach(() => {
  motion.reduced = false;
  motion.listeners.clear();
});
