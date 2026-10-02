import test from "node:test";
import assert from "node:assert/strict";

globalThis.chrome = { runtime: {} };
const native = await import("../extension/native.js");

test("stepOpacity moves in 10% steps and stays in range", () => {
  assert.equal(native.stepOpacity(1, -1), 0.9);
  assert.equal(native.stepOpacity(0.9, +1), 1);
  assert.equal(native.stepOpacity(1, +1), 1);
  assert.equal(native.stepOpacity(0.2, -1), native.MIN_OPACITY);
  assert.equal(native.stepOpacity(0.55, -1), 0.5); // snaps to the grid
});

test("call reports a missing native host", async () => {
  chrome.runtime.sendNativeMessage = async () => {
    throw new Error("Specified native messaging host not found.");
  };
  assert.deepEqual(await native.getState(), {
    ok: false, code: "host_missing", error: "The PiP Anywhere helper is not installed.",
  });
});

test("call passes requests and replies through", async () => {
  let sent;
  chrome.runtime.sendNativeMessage = async (host, msg) => {
    sent = [host, msg];
    return { ok: true, window: { above: true, opacity: 0.5 } };
  };
  assert.equal((await native.setOpacity(0.5)).window.opacity, 0.5);
  assert.deepEqual(sent, [native.HOST, { cmd: "set_opacity", opacity: 0.5 }]);
  await native.setAbove(true);
  assert.deepEqual(sent[1], { cmd: "set_above", above: true });
});

test("helperStatus accepts a current helper", () => {
  const v = native.REQUIRED_HELPER_VERSION;
  assert.deepEqual(native.helperStatus({ ok: true, versions: { host: v, helper: v, helper_installed: v } }, "linux"), { state: "ok" });
  // Windows: the host is the helper.
  assert.deepEqual(native.helperStatus({ ok: true, versions: { host: v, helper: v, helper_installed: v } }, "win"), { state: "ok" });
});

test("helperStatus asks to log in again when GNOME still runs the old extension", () => {
  const v = native.REQUIRED_HELPER_VERSION;
  const s = native.helperStatus({ ok: true, versions: { host: v, helper: v - 1, helper_installed: v } }, "linux");
  assert.equal(s.state, "relogin");
});

test("helperStatus asks to reinstall an old host or helper", () => {
  const v = native.REQUIRED_HELPER_VERSION;
  assert.equal(native.helperStatus({ ok: true, window: {} }, "linux").state, "reinstall"); // host before v4 sends no versions
  assert.equal(native.helperStatus({ ok: true, versions: { host: v - 1, helper: v, helper_installed: v } }, "linux").state, "reinstall");
  assert.equal(native.helperStatus({ ok: true, versions: { host: v, helper: v - 1, helper_installed: v - 1 } }, "linux").state, "reinstall");
  assert.match(native.helperStatus({ ok: true }, "win").message, /install\.cmd/);
});

test("helperStatus leaves a missing host to the setup screen", () => {
  assert.deepEqual(native.helperStatus({ ok: false, code: "host_missing" }, "linux"), { state: "ok" });
  // Helper not running yet: versions are unknown but the host is current.
  const v = native.REQUIRED_HELPER_VERSION;
  assert.deepEqual(native.helperStatus({ ok: false, code: "helper_missing", versions: { host: v, helper: null, helper_installed: v } }, "linux"), { state: "ok" });
});

test("setOpacity can carry the Clear on hover preference", async () => {
  let sent;
  chrome.runtime.sendNativeMessage = async (_host, msg) => {
    sent = msg;
    return { ok: true };
  };
  await native.setOpacity(0.6, true);
  assert.deepEqual(sent, { cmd: "set_opacity", opacity: 0.6, hover_reveal: true });
  await native.setOpacity(0.6);
  assert.deepEqual(sent, { cmd: "set_opacity", opacity: 0.6 });
  await native.setHoverReveal(false);
  assert.deepEqual(sent, { cmd: "set_hover_reveal", enabled: false });
});

test("Clear on hover preference defaults to on and is stored", async () => {
  const store = {};
  chrome.storage = {
    sync: {
      get: async (key) => (key in store ? { [key]: store[key] } : {}),
      set: async (items) => Object.assign(store, items),
    },
  };
  assert.equal(await native.getHoverRevealPref(), true);
  await native.saveHoverRevealPref(false);
  assert.equal(await native.getHoverRevealPref(), false);
});

test("supportsHoverReveal depends on the helper", () => {
  assert.equal(native.supportsHoverReveal({ ok: true, window: { hover_reveal: false } }), true);
  assert.equal(native.supportsHoverReveal({ ok: true, window: {} }), false); // Windows helper
  assert.equal(native.supportsHoverReveal({ ok: false }), false);
});
