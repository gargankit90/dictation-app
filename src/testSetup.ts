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
