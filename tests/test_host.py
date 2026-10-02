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

    def test_old_running_helper_is_reported(self):
        def runner(cmd, **_):
            return subprocess.CompletedProcess(cmd, 1, "", "GDBus.Error:org.freedesktop.DBus.Error.UnknownMethod: No such method")
        with self.assertRaises(host.HostError) as ctx:
            host.gdbus_call("SetHoverReveal", "true", runner=runner)
        self.assertEqual(ctx.exception.code, "helper_outdated")
        self.assertIn("Log out and back in", str(ctx.exception))

    def test_builds_gdbus_command(self):
        seen = {}
        def runner(cmd, **_):
            seen["cmd"] = cmd
            return subprocess.CompletedProcess(cmd, 0, "('{\"ok\": true}',)\n", "")
        self.assertEqual(host.gdbus_call("SetOpacity", "0.500", runner=runner), {"ok": True})
        self.assertIn("com.pipanywhere.Shell.SetOpacity", seen["cmd"])
        self.assertEqual(seen["cmd"][-1], "0.500")


class VersionPropertyTest(unittest.TestCase):
    def test_parses_uint32_property(self):
        def runner(cmd, **_):
            return subprocess.CompletedProcess(cmd, 0, "(<uint32 4>,)\n", "")
        self.assertEqual(host.gdbus_version(runner=runner), 4)  # not the "32" in "uint32"

    def test_unknown_when_property_is_missing(self):
        def runner(cmd, **_):
            return subprocess.CompletedProcess(cmd, 1, "", "No such interface")
        self.assertIsNone(host.gdbus_version(runner=runner))


class HandleTest(unittest.TestCase):
    def setUp(self):
        self.calls = []

    def call(self, method, *args):
        self.calls.append((method, args))
        if method == "Diagnostics":
            return {"version": 4, "stack": []}
        return {"ok": True, "version": 4, "window": {"above": method == "SetAbove"}}

    def handle(self, request, call=None, running=lambda: 3, installed=lambda: 4):
        return host.handle(request, call or self.call, running_version=running, installed_version=installed)

    def test_ping(self):
        self.assertEqual(self.handle({"cmd": "ping"}), {
            "ok": True, "versions": {"host": host.VERSION, "helper": 3, "helper_installed": 4}})

    def test_versions_come_from_the_helper_reply(self):
        res = self.handle({"cmd": "state"}, running=lambda: self.fail("must not query D-Bus"))
        self.assertEqual(res["versions"], {"host": host.VERSION, "helper": 4, "helper_installed": 4})
        self.assertNotIn("version", res)

    def test_old_helper_without_version_in_reply(self):
        res = self.handle({"cmd": "state"}, call=lambda *_: {"ok": True, "window": {}})
        self.assertEqual(res["versions"]["helper"], 3)  # from the D-Bus Version property

    def test_commands_map_to_dbus_methods(self):
        self.handle({"cmd": "state"})
        self.handle({"cmd": "set_above", "above": True})
        self.handle({"cmd": "set_above", "above": False})
        self.handle({"cmd": "set_opacity", "opacity": 0.5})
        self.assertEqual(self.calls, [
            ("GetFocused", ()),
            ("SetAbove", ("true",)),
            ("SetAbove", ("false",)),
            ("SetOpacity", ("0.500",)),
        ])

    def test_hover_reveal(self):
        self.handle({"cmd": "set_hover_reveal", "enabled": True})
        self.handle({"cmd": "set_hover_reveal", "enabled": False})
        self.handle({"cmd": "set_opacity", "opacity": 0.6, "hover_reveal": True})
        self.handle({"cmd": "set_opacity", "opacity": 0.6})
        self.assertEqual(self.calls, [
            ("SetHoverReveal", ("true",)),
            ("SetHoverReveal", ("false",)),
            ("SetOpacity", ("0.600",)), ("SetHoverReveal", ("true",)),
            ("SetOpacity", ("0.600",)),
        ])

    def test_opacity_is_clamped(self):
        self.handle({"cmd": "set_opacity", "opacity": 0})
        self.handle({"cmd": "set_opacity", "opacity": 7})
        self.assertEqual([a for _, a in self.calls], [("0.200",), ("1.000",)])

    def test_bad_requests(self):
        for req in [[], {"cmd": "nope"}, {"cmd": "set_opacity", "opacity": "x"}, {"cmd": "set_opacity", "opacity": float("nan")}]:
            res = self.handle(req, running=lambda: self.fail("must not query D-Bus"))
            self.assertFalse(res["ok"])
            self.assertEqual(res["code"], "bad_request")
        self.assertEqual(self.calls, [])

    def test_no_focused_window(self):
        res = self.handle({"cmd": "state"}, call=lambda *_: {"ok": False, "error": "No focused window.", "version": 4})
        self.assertEqual(res, {"ok": False, "error": "No focused window.", "code": "no_window",
                               "versions": {"host": host.VERSION, "helper": 4, "helper_installed": 4}})

    def test_missing_helper_keeps_versions(self):
        def call(*_):
            raise host.HostError("not running", "helper_missing")
        res = self.handle({"cmd": "state"}, call=call, running=lambda: self.fail("must not query D-Bus"))
        self.assertEqual(res["code"], "helper_missing")
        self.assertEqual(res["versions"], {"host": host.VERSION, "helper": None, "helper_installed": 4})

    def test_diagnostics(self):
        res = self.handle({"cmd": "diagnostics"})
        self.assertTrue(res["ok"])
        report = res["diagnostics"]
        self.assertEqual(report["host"]["version"], host.VERSION)
        self.assertIn("log_tail", report["host"])
        self.assertEqual(report["gnome"], {"version": 4, "stack": []})
        self.assertEqual(res["versions"]["helper"], 4)

    def test_diagnostics_without_helper(self):
        def call(*_):
            raise host.HostError("not running", "helper_missing")
        res = self.handle({"cmd": "diagnostics"}, call=call)
        self.assertTrue(res["ok"])
        self.assertEqual(res["diagnostics"]["gnome_error"], "not running")


class ProcessTest(unittest.TestCase):
    def test_runs_as_native_host(self):
        buf = io.BytesIO()
        host.write_message(buf, {"cmd": "ping"})
        with tempfile.TemporaryDirectory() as state:
            env = {**os.environ, "XDG_STATE_HOME": state, "XDG_DATA_HOME": state}
            out = subprocess.run([sys.executable, str(Path(host.__file__))], input=buf.getvalue(),
                                 capture_output=True, timeout=10, env=env)
            reply = host.read_message(io.BytesIO(out.stdout))
            self.assertTrue(reply["ok"])
            self.assertEqual(reply["versions"]["host"], host.VERSION)
            log = (Path(state) / "pip-anywhere" / "host.log").read_text()
            # Each line starts with a full ISO date and time, e.g. 2026-10-02T14:02:24.739+07:00.
            self.assertRegex(log, r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[+-]\d{2}:\d{2} ")
            self.assertIn('{"cmd": "ping"} -> {"ok": true', log)


if __name__ == "__main__":
    unittest.main()
