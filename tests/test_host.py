import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "native"))
import host  # noqa: E402


class FramingTest(unittest.TestCase):
    def test_round_trip(self):
        buf = io.BytesIO()
        host.write_message(buf, {"cmd": "state", "text": "Tiếng Việt"})
        buf.seek(0)
        self.assertEqual(host.read_message(buf), {"cmd": "state", "text": "Tiếng Việt"})
        self.assertIsNone(host.read_message(buf))


class GdbusParsingTest(unittest.TestCase):
    def test_parses_string_reply(self):
        # As printed by gdbus: single quotes escaped, backslashes doubled.
        title = "It's \"quoted\" – tab"
        reply = json.dumps({"ok": True, "window": {"title": title}}, ensure_ascii=False)
        out = "('" + reply.replace("\\", "\\\\").replace("'", "\\'") + "',)\n"
        parsed = host.parse_gdbus_string(out)
        self.assertEqual(parsed, reply)
        self.assertEqual(json.loads(parsed)["window"]["title"], title)

    def test_parses_double_quoted_reply(self):
        self.assertEqual(host.parse_gdbus_string('("{\\"ok\\": true}",)'), '{"ok": true}')

    def test_rejects_unexpected_output(self):
        with self.assertRaises(host.HostError):
            host.parse_gdbus_string("(true,)")

    def test_missing_helper_is_reported(self):
        def runner(cmd, **_):
            return subprocess.CompletedProcess(cmd, 1, "", "Error: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown")
        with self.assertRaises(host.HostError) as ctx:
            host.gdbus_call("GetFocused", runner=runner)
        self.assertEqual(ctx.exception.code, "helper_missing")

    def test_builds_gdbus_command(self):
        seen = {}
        def runner(cmd, **_):
            seen["cmd"] = cmd
            return subprocess.CompletedProcess(cmd, 0, "('{\"ok\": true}',)\n", "")
        self.assertEqual(host.gdbus_call("SetOpacity", "0.500", runner=runner), {"ok": True})
        self.assertIn("com.pipanywhere.Shell.SetOpacity", seen["cmd"])
        self.assertEqual(seen["cmd"][-1], "0.500")


class HandleTest(unittest.TestCase):
    def setUp(self):
        self.calls = []

    def call(self, method, *args):
        self.calls.append((method, args))
        return {"ok": True, "window": {"above": method == "SetAbove"}}

    def test_ping(self):
        self.assertEqual(host.handle({"cmd": "ping"}, self.call), {"ok": True, "version": host.VERSION})

    def test_commands_map_to_dbus_methods(self):
        host.handle({"cmd": "state"}, self.call)
        host.handle({"cmd": "set_above", "above": True}, self.call)
        host.handle({"cmd": "set_above", "above": False}, self.call)
        host.handle({"cmd": "set_opacity", "opacity": 0.5}, self.call)
        self.assertEqual(self.calls, [
            ("GetFocused", ()),
            ("SetAbove", ("true",)),
            ("SetAbove", ("false",)),
            ("SetOpacity", ("0.500",)),
        ])

    def test_opacity_is_clamped(self):
        host.handle({"cmd": "set_opacity", "opacity": 0}, self.call)
        host.handle({"cmd": "set_opacity", "opacity": 7}, self.call)
        self.assertEqual([a for _, a in self.calls], [("0.200",), ("1.000",)])

    def test_bad_requests(self):
        for req in [[], {"cmd": "nope"}, {"cmd": "set_opacity", "opacity": "x"}, {"cmd": "set_opacity", "opacity": float("nan")}]:
            res = host.handle(req, self.call)
            self.assertFalse(res["ok"])
            self.assertEqual(res["code"], "bad_request")
        self.assertEqual(self.calls, [])

    def test_no_focused_window(self):
        res = host.handle({"cmd": "state"}, lambda *_: {"ok": False, "error": "No focused window."})
        self.assertEqual(res, {"ok": False, "error": "No focused window.", "code": "no_window"})


class ProcessTest(unittest.TestCase):
    def test_runs_as_native_host(self):
        buf = io.BytesIO()
        host.write_message(buf, {"cmd": "ping"})
        with tempfile.TemporaryDirectory() as state:
            env = {**os.environ, "XDG_STATE_HOME": state}
            out = subprocess.run([sys.executable, str(Path(host.__file__))], input=buf.getvalue(),
                                 capture_output=True, timeout=10, env=env)
            self.assertEqual(host.read_message(io.BytesIO(out.stdout)), {"ok": True, "version": host.VERSION})
            log = (Path(state) / "pip-anywhere" / "host.log").read_text()
            self.assertIn('{"cmd": "ping"} -> {"ok": true', log)


if __name__ == "__main__":
    unittest.main()
