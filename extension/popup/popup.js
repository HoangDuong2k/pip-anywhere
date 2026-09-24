import { getState, setAbove, setOpacity } from "../native.js";

const $ = (id) => document.getElementById(id);
const controls = $("controls");
const above = $("above");
const opacity = $("opacity");
const opacityValue = $("opacity-value");
const error = $("error");

function showError(message) {
  error.textContent = message;
  error.hidden = !message;
}

// Setup instructions depend on the operating system.
const platform = chrome.runtime.getPlatformInfo().then(({ os }) => (os === "win" || os === "linux" ? os : "other"));

async function showSetup(message) {
  const os = await platform;
  $("setup-message").textContent = message;
  for (const el of document.querySelectorAll("[data-os]")) el.hidden = el.dataset.os !== os;
  $("setup").hidden = false;
}

function render(state) {
  if (!state.ok) {
    controls.hidden = true;
    if (state.code === "host_missing" || state.code === "helper_missing" || state.code === "unsupported") {
      showSetup(state.error);
      showError("");
    } else {
      showError(state.error);
    }
    return;
  }
  const win = state.window;
  $("setup").hidden = true;
  controls.hidden = false;
  showError("");
  $("window-title").textContent = win.title || win.wm_class;
  $("window-title").title = `${win.title} (${win.wm_class})`;
  above.checked = win.above;
  // Do not fight the user while they are dragging the slider.
  if (document.activeElement !== opacity) opacity.value = String(Math.round(win.opacity * 100));
  opacityValue.textContent = `${opacity.value}%`;
}

above.addEventListener("change", async () => render(await setAbove(above.checked)));

// Each request starts the native host, so send at most one at a time and always the latest value.
let pending = null;
let inFlight = false;
async function flushOpacity() {
  if (inFlight || pending === null) return;
  inFlight = true;
  const value = pending;
  pending = null;
  const state = await setOpacity(value);
  inFlight = false;
  if (!state.ok) render(state);
  flushOpacity();
}
opacity.addEventListener("input", () => {
  opacityValue.textContent = `${opacity.value}%`;
  pending = Number(opacity.value) / 100;
  flushOpacity();
});

chrome.commands.getAll().then((commands) => {
  for (const kbd of document.querySelectorAll("kbd[data-command]")) {
    kbd.textContent = commands.find((c) => c.name === kbd.dataset.command)?.shortcut || "—";
  }
});
$("shortcuts").addEventListener("click", (e) => {
  e.preventDefault();
  chrome.tabs.create({ url: "chrome://extensions/shortcuts" });
});

getState().then(render);
