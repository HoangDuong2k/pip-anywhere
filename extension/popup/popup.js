import {
  getDiagnostics,
  getHoverRevealPref,
  getState,
  helperStatus,
  saveHoverRevealPref,
  setAbove,
  setHoverReveal,
  setOpacity,
  supportsHoverReveal,
} from "../native.js";

const $ = (id) => document.getElementById(id);
const controls = $("controls");
const above = $("above");
const opacity = $("opacity");
const opacityValue = $("opacity-value");
const error = $("error");
const hoverReveal = $("hover-reveal");
const hoverPref = getHoverRevealPref().then((enabled) => (hoverReveal.checked = enabled));

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

let lastState = null;

async function showHelperStatus(state) {
  const status = helperStatus(state, await platform);
  $("warning").textContent = status.message ?? "";
  $("warning").hidden = status.state === "ok";
}

function render(state) {
  lastState = state;
  showHelperStatus(state);
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
  // Only shown when the helper can do it (GNOME helper 5+; not Windows yet).
  $("hover-row").hidden = !supportsHoverReveal(state);
}

above.addEventListener("change", async () => render(await setAbove(above.checked)));

hoverReveal.addEventListener("change", async () => {
  await saveHoverRevealPref(hoverReveal.checked);
  render(await setHoverReveal(hoverReveal.checked));
});

// Each request starts the native host, so send at most one at a time and always the latest value.
let pending = null;
let inFlight = false;
async function flushOpacity() {
  if (inFlight || pending === null) return;
  inFlight = true;
  const value = pending;
  pending = null;
  const state = await setOpacity(value, supportsHoverReveal(lastState) ? hoverReveal.checked : undefined);
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

// Show the window state; send the saved preference to the helper, which forgets it at logout.
getState().then(async (state) => {
  render(state);
  if (supportsHoverReveal(state)) {
    await hoverPref;
    if (state.window.hover_reveal !== hoverReveal.checked) render(await setHoverReveal(hoverReveal.checked));
  }
});

// A report the user can paste into a bug report.
async function diagnosticsReport() {
  const reply = await getDiagnostics();
  return JSON.stringify(
    {
      generated: new Date().toISOString(),
      extension_version: chrome.runtime.getManifest().version,
      browser: navigator.userAgent,
      platform: await chrome.runtime.getPlatformInfo(),
      last_state: lastState,
      helper: reply,
    },
    null,
    2,
  );
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = Object.assign(document.createElement("textarea"), { value: text });
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

$("diagnostics").addEventListener("click", async () => {
  const status = $("diagnostics-status");
  status.textContent = "Collecting…";
  try {
    await copyText(await diagnosticsReport());
    status.textContent = "Copied. Paste it into your bug report.";
  } catch (e) {
    status.textContent = `Could not copy: ${e.message ?? e}`;
  }
});
