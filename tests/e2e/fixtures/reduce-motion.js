// Sets Reduce Motion in System Settings' open Display pane: on or off. The switch
// has no name of its own, so it's the first switch after its text. Prints the state.
function run(argv) {
  const want = argv[0] === "on" ? 1 : 0;
  const window = Application("System Events").processes.byName("System Settings").windows[0];
  const all = window.entireContents();
  let label = -1;
  for (let i = 0; i < all.length; i += 1) {
    const element = all[i];
    if (label < 0) {
      if (element.role() === "AXStaticText" && String(element.value()) === "Reduce motion") {
        label = i;
      }
    } else if (["AXCheckBox", "AXSwitch"].includes(element.role())) {
      if (Number(element.value()) !== want) element.click();
      delay(1);
      return Number(element.value()) === want ? (want ? "on" : "off") : "unchanged";
    }
  }
  throw new Error(label < 0 ? "no Reduce motion text" : "no switch after Reduce motion");
}
