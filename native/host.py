#!/usr/bin/env python3
"""PiP Anywhere native messaging host.

Chrome starts this script for each request from the extension (chrome.runtime.sendNativeMessage)
and talks to it over stdin/stdout: every message is a 32-bit native-endian length followed by
UTF-8 JSON. The host forwards the request to the desktop:

- GNOME (Wayland or X11): the "PiP Anywhere helper" GNOME Shell extension over D-Bus.

Requests:  {"cmd": "ping"} | {"cmd": "state"} | {"cmd": "set_above", "above": bool}
           | {"cmd": "set_opacity", "opacity": float, "hover_reveal"?: bool}
           | {"cmd": "set_hover_reveal", "enabled": bool} | {"cmd": "diagnostics"}
Responses: {"ok": true, "window": {...}, "versions": {...}}
           | {"ok": false, "error": "...", "code": "...", "versions": {...}}

`versions` = {"host": this script, "helper": running GNOME extension, "helper_installed": the
GNOME extension on disk}. The browser extension uses it to tell the user to reinstall or to log
out and back in (GNOME only loads new extension code at login).
"""

import ast
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

VERSION = 5
UUID = "pip-anywhere@pipanywhere.github.io"
BUS_NAME = "com.pipanywhere.Shell"
OBJECT_PATH = "/com/pipanywhere/Shell"
INTERFACE = "com.pipanywhere.Shell"
MIN_OPACITY = 0.2


class HostError(Exception):
    def __init__(self, message, code="error"):
        super().__init__(message)
        self.code = code


# ---------------------------------------------------------------- native messaging framing

def read_message(stream):
    """Reads one message; returns None at end of input."""
    header = stream.read(4)
    if len(header) < 4:
        return None
    (length,) = struct.unpack("=I", header)
    return json.loads(stream.read(length).decode("utf-8"))


def write_message(stream, message):
    data = json.dumps(message).encode("utf-8")
    stream.write(struct.pack("=I", len(data)))
    stream.write(data)
    stream.flush()


# ---------------------------------------------------------------- GNOME backend

def parse_gdbus_string(output):
    """Extracts the string from gdbus call output such as `('{"ok": true}',)`."""
    text = output.strip()
    # A one-string GVariant tuple in text form is also a valid Python tuple literal
    # (same quoting and \\, \\', \\n, \\uXXXX escapes).
    try:
        value = ast.literal_eval(text)
    except (ValueError, SyntaxError):
        value = None
    if not (isinstance(value, tuple) and len(value) == 1 and isinstance(value[0], str)):
        raise HostError(f"unexpected D-Bus reply: {text[:200]}")
    return value[0]


def gdbus_call(method, *args, runner=subprocess.run):
    if not shutil.which("gdbus"):
        raise HostError("gdbus was not found (is this a GNOME desktop?).", "unsupported")
    cmd = ["gdbus", "call", "--session", "--dest", BUS_NAME, "--object-path", OBJECT_PATH,
           "--method", f"{INTERFACE}.{method}", *args]
    result = runner(cmd, capture_output=True, text=True, timeout=5)
    if result.returncode != 0:
        if "ServiceUnknown" in result.stderr or "was not provided" in result.stderr:
            raise HostError(
                "The PiP Anywhere GNOME extension is not running. Log out and back in after "
                "installing, then run: gnome-extensions enable pip-anywhere@pipanywhere.github.io",
                "helper_missing")
        if "UnknownMethod" in result.stderr:
            raise HostError(
                "The PiP Anywhere GNOME extension running now is older than the installed one. "
                "Log out and back in to load the new version.",
                "helper_outdated")
        raise HostError(result.stderr.strip() or "D-Bus call failed")
    return json.loads(parse_gdbus_string(result.stdout))


def gdbus_version(runner=subprocess.run):
    """Version property of the running GNOME extension (for versions that do not report it)."""
    result = runner(["gdbus", "call", "--session", "--dest", BUS_NAME, "--object-path", OBJECT_PATH,
                     "--method", "org.freedesktop.DBus.Properties.Get", INTERFACE, "Version"],
                    capture_output=True, text=True, timeout=5)
    # Output looks like `(<uint32 4>,)`.
    match = re.search(r"<uint32 (\d+)>", result.stdout) if result.returncode == 0 else None
    return int(match.group(1)) if match else None


def installed_helper_version():
    data = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local" / "share")
    try:
        meta = Path(data) / "gnome-shell" / "extensions" / UUID / "metadata.json"
        return int(json.loads(meta.read_text())["version"])
    except (OSError, ValueError, KeyError, TypeError):
        return None


def handle(request, call=gdbus_call, running_version=gdbus_version, installed_version=installed_helper_version):
    """Handles one request and returns the response (never raises)."""
    response = _handle(request, call)
    helper = response.pop("version", None)
    if helper is None and response.get("code") not in ("helper_missing", "unsupported", "bad_request"):
        try:
            helper = running_version()
        except Exception:
            helper = None
    response["versions"] = {"host": VERSION, "helper": helper, "helper_installed": installed_version()}
    return response


def _handle(request, call):
    try:
        if not isinstance(request, dict):
            raise HostError("request must be a JSON object", "bad_request")
        cmd = request.get("cmd")
        if cmd == "ping":
            return {"ok": True}
        if cmd == "diagnostics":
            report = diagnostics(call)
            return {"ok": True, "diagnostics": report, "version": (report.get("gnome") or {}).get("version")}
        if cmd == "state":
            reply = call("GetFocused")
        elif cmd == "set_above":
            reply = call("SetAbove", "true" if request.get("above") else "false")
        elif cmd == "set_opacity":
            try:
                opacity = float(request.get("opacity"))
            except (TypeError, ValueError):
                raise HostError("opacity must be a number", "bad_request")
            if opacity != opacity:  # NaN
                raise HostError("opacity must be a number", "bad_request")
            opacity = min(max(opacity, MIN_OPACITY), 1.0)
            reply = call("SetOpacity", f"{opacity:.3f}")
            # The extension sends its "Clear on hover" preference along, so it also applies
            # after GNOME restarted (the helper keeps no settings across logins).
            if "hover_reveal" in request and reply.get("ok"):
                reply = call("SetHoverReveal", "true" if request["hover_reveal"] else "false")
        elif cmd == "set_hover_reveal":
            reply = call("SetHoverReveal", "true" if request.get("enabled") else "false")
        else:
            raise HostError(f"unknown command: {cmd!r}", "bad_request")
        if not reply.get("ok"):
            return {"ok": False, "error": reply.get("error", "failed"), "code": "no_window",
                    "version": reply.get("version")}
        return reply
    except HostError as e:
        return {"ok": False, "error": str(e), "code": e.code}
    except Exception as e:  # keep the extension informed instead of dying silently
        return {"ok": False, "error": f"{type(e).__name__}: {e}", "code": "error"}


def diagnostics(call):
    """Host, desktop and window-stack details for bug reports (window titles are truncated)."""
    report = {
        "host": {
            "version": VERSION,
            "python": platform.python_version(),
            "platform": platform.platform(),
            "desktop": os.environ.get("XDG_CURRENT_DESKTOP"),
            "session": os.environ.get("XDG_SESSION_TYPE"),
            "helper_installed": installed_helper_version(),
            "log_tail": log_tail(40),
        },
    }
    try:
        report["gnome"] = call("Diagnostics")
    except HostError as e:
        report["gnome_error"] = str(e)
    return report


LOG_MAX_BYTES = 256 * 1024


def log_path():
    state = os.environ.get("XDG_STATE_HOME") or str(Path.home() / ".local" / "state")
    return Path(state) / "pip-anywhere" / "host.log"


def log(request, response, elapsed):
    """Appends one line per request, for troubleshooting. Never fails the request."""
    try:
        path = log_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists() and path.stat().st_size > LOG_MAX_BYTES:
            path.replace(path.with_suffix(".log.1"))
        stamp = datetime.now().astimezone().isoformat(timespec="milliseconds")
        if isinstance(request, dict) and request.get("cmd") == "diagnostics":
            response = {"ok": response.get("ok")}  # the report itself is large and already returned
        with path.open("a", encoding="utf-8") as f:
            f.write(f"{stamp} {elapsed * 1000:.0f}ms {json.dumps(request)} -> {json.dumps(response, ensure_ascii=False)}\n")
    except OSError:
        pass


def log_tail(lines):
    try:
        return log_path().read_text(encoding="utf-8").splitlines()[-lines:]
    except OSError:
        return []


def main():
    stdin, stdout = sys.stdin.buffer, sys.stdout.buffer
    while True:
        message = read_message(stdin)
        if message is None:
            return
        start = time.monotonic()
        response = handle(message)
        write_message(stdout, response)
        log(message, response, time.monotonic() - start)


if __name__ == "__main__":
    main()
