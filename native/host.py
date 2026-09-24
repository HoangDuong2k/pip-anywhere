#!/usr/bin/env python3
"""PiP Anywhere native messaging host.

Chrome starts this script for each request from the extension (chrome.runtime.sendNativeMessage)
and talks to it over stdin/stdout: every message is a 32-bit native-endian length followed by
UTF-8 JSON. The host forwards the request to the desktop:

- GNOME (Wayland or X11): the "PiP Anywhere helper" GNOME Shell extension over D-Bus.

Requests:  {"cmd": "state"} | {"cmd": "set_above", "above": bool} | {"cmd": "set_opacity", "opacity": float}
Responses: {"ok": true, "window": {...}} | {"ok": false, "error": "...", "code": "..."}
"""

import ast
import json
import os
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

VERSION = 2
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
        raise HostError(result.stderr.strip() or "D-Bus call failed")
    return json.loads(parse_gdbus_string(result.stdout))


def handle(request, call=gdbus_call):
    """Handles one request and returns the response (never raises)."""
    try:
        if not isinstance(request, dict):
            raise HostError("request must be a JSON object", "bad_request")
        cmd = request.get("cmd")
        if cmd == "ping":
            return {"ok": True, "version": VERSION}
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
        else:
            raise HostError(f"unknown command: {cmd!r}", "bad_request")
        if not reply.get("ok"):
            return {"ok": False, "error": reply.get("error", "failed"), "code": "no_window"}
        return reply
    except HostError as e:
        return {"ok": False, "error": str(e), "code": e.code}
    except Exception as e:  # keep the extension informed instead of dying silently
        return {"ok": False, "error": f"{type(e).__name__}: {e}", "code": "error"}


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
        stamp = time.strftime("%H:%M:%S") + f".{int(time.time() * 1000) % 1000:03d}"
        with path.open("a", encoding="utf-8") as f:
            f.write(f"{stamp} {elapsed * 1000:.0f}ms {json.dumps(request)} -> {json.dumps(response, ensure_ascii=False)}\n")
    except OSError:
        pass


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
