// jsdom doesn't implement scrollIntoView; the language picker calls it.
if (typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = () => undefined;
}
