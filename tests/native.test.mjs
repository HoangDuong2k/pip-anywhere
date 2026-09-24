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
