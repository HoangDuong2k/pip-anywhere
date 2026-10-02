// Talks to the native host (native/host.py), which acts on the focused browser window.
export const HOST = "com.pipanywhere.host";
export const OPACITY_STEP = 0.1;
export const MIN_OPACITY = 0.2;
/** Oldest helper (native host and GNOME extension) this extension works with. */
export const REQUIRED_HELPER_VERSION = 5;

/**
 * Sends one request. Always resolves to `{ ok: true, window }` or `{ ok: false, error, code }`;
 * `code` is "host_missing" when the native host is not installed.
 */
export async function call(request) {
  try {
    return await chrome.runtime.sendNativeMessage(HOST, request);
  } catch (e) {
    const message = String(e?.message ?? e);
    if (/not found|forbidden/i.test(message)) {
      return { ok: false, code: "host_missing", error: "The PiP Anywhere helper is not installed." };
    }
    return { ok: false, code: "error", error: message };
  }
}

export const getState = () => call({ cmd: "state" });
export const setAbove = (above) => call({ cmd: "set_above", above });
/** `hoverReveal` (optional) re-sends the "Clear on hover" preference with the change. */
export const setOpacity = (opacity, hoverReveal) =>
  call(hoverReveal === undefined ? { cmd: "set_opacity", opacity } : { cmd: "set_opacity", opacity, hover_reveal: hoverReveal });
export const setHoverReveal = (enabled) => call({ cmd: "set_hover_reveal", enabled });

/** "Clear on hover" preference, stored in chrome.storage.sync (on by default). */
export async function getHoverRevealPref() {
  const { hoverReveal = true } = await chrome.storage.sync.get("hoverReveal");
  return hoverReveal;
}
export const saveHoverRevealPref = (enabled) => chrome.storage.sync.set({ hoverReveal: enabled });

/** Whether the helper supports "Clear on hover" (GNOME helper 5+; not Windows yet). */
export const supportsHoverReveal = (state) => typeof state?.window?.hover_reveal === "boolean";
export const getDiagnostics = () => call({ cmd: "diagnostics" });

/**
 * Whether the installed helper is new enough, from the `versions` the host adds to every reply.
 * Returns `{ state: "ok" }`, or `{ state: "reinstall" | "relogin", message }`.
 */
export function helperStatus(reply, os) {
  if (!reply || reply.code === "host_missing") return { state: "ok" }; // the setup screen covers it
  const v = reply.versions;
  const reinstall = {
    state: "reinstall",
    message:
      os === "win"
        ? "The helper is out of date. Run install.cmd from the latest pip-anywhere-windows.zip."
        : "The helper is out of date. Run ./scripts/install-linux.sh again.",
  };
  if (!v || !(v.host >= REQUIRED_HELPER_VERSION)) return reinstall;
  if (v.helper != null && v.helper < REQUIRED_HELPER_VERSION) {
    return v.helper_installed >= REQUIRED_HELPER_VERSION
      ? { state: "relogin", message: "The helper was updated. Log out and back in to finish (GNOME loads extensions at login)." }
      : reinstall;
  }
  return { state: "ok" };
}

/** Next opacity for the keyboard shortcuts, snapped to 10% steps. */
export function stepOpacity(current, direction) {
  const next = Math.round((current + direction * OPACITY_STEP) * 10) / 10;
  return Math.min(1, Math.max(MIN_OPACITY, next));
}
