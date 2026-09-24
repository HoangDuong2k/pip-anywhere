// Talks to the native host (native/host.py), which acts on the focused browser window.
export const HOST = "com.pipanywhere.host";
export const OPACITY_STEP = 0.1;
export const MIN_OPACITY = 0.2;

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
export const setOpacity = (opacity) => call({ cmd: "set_opacity", opacity });

/** Next opacity for the keyboard shortcuts, snapped to 10% steps. */
export function stepOpacity(current, direction) {
  const next = Math.round((current + direction * OPACITY_STEP) * 10) / 10;
  return Math.min(1, Math.max(MIN_OPACITY, next));
}
