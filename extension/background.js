import { getHoverRevealPref, getState, setAbove, setOpacity, stepOpacity, supportsHoverReveal } from "./native.js";

// Keyboard shortcuts act on the focused browser window.
chrome.commands.onCommand.addListener(async (command) => {
  const state = await getState();
  if (!state.ok) {
    console.warn("[PiP Anywhere]", state.error);
    return;
  }
  const { above, opacity } = state.window;
  const hoverReveal = supportsHoverReveal(state) ? await getHoverRevealPref() : undefined;
  if (command === "toggle-on-top") await setAbove(!above);
  else if (command === "opacity-down") await setOpacity(stepOpacity(opacity, -1), hoverReveal);
  else if (command === "opacity-up") await setOpacity(stepOpacity(opacity, +1), hoverReveal);
});
