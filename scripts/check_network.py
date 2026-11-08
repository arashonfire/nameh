#!/usr/bin/env python3
"""Trace Linux x86-64 release opening with ptrace, including worker descendants.

Requires cc, Hyprland, gdbus, desktop access and permission to trace own children.
Allows Unix desktop IPC; rejects other socket families and uninspected io_uring.
Checks the tracer against AF_UNIX/AF_INET controls before testing nameh. Never
activates external links. This does not trace unrelated desktop daemon activity.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.uname().machine != "x86_64":
        raise RuntimeError("tracer supports only Linux x86-64")
    if subprocess.run(["pgrep", "-x", "nameh|mdview"], capture_output=True).returncode == 0:
        raise RuntimeError("close existing nameh instances first")
    if args.output.exists():
        raise RuntimeError("use a new output path")
    with tempfile.TemporaryDirectory(prefix="nameh-network-") as temporary:
        directory = Path(temporary)
        tracer = directory / "trace"
        subprocess.run(["cc", "-O2", "-Wall", "-Wextra", "-Werror",
                        str(ROOT / "scripts/trace_network.c"), "-o", str(tracer)], check=True)
        for family, expected in [("AF_UNIX", 0), ("AF_INET", 1)]:
            control = subprocess.run([str(tracer), str(directory / (family + ".log")), sys.executable,
                                      "-c", f"import socket; socket.socket(socket.{family}).close()"], timeout=10)
            if control.returncode != expected:
                raise RuntimeError(f"tracer control {family} failed: {control.returncode}; ptrace may be unavailable")
        config = directory / "config"
        config.mkdir()
        gtk_config = Path.home() / ".config/gtk-4.0"
        if gtk_config.exists():
            (config / "gtk-4.0").symlink_to(gtk_config)
        document = directory / "network café.md"
        source = (ROOT / "fixtures/rendering.md").read_bytes() + b'''

![Blocked HTTP](http://127.0.0.1:9/blocked.png)
![Blocked HTTPS](https://example.invalid/blocked.png)
[Unactivated web](https://example.invalid/)
[Unactivated mail](mailto:test@example.invalid)
<img src="https://example.invalid/raw.png">
'''
        document.write_bytes(source)
        for name in ["assets", "notes"]:
            (directory / name).symlink_to(ROOT / "fixtures" / name)
        trace = args.output.with_suffix(".log").resolve()
        binary = ROOT / "target/release/nameh"
        env = dict(os.environ, XDG_CONFIG_HOME=str(config), GDK_BACKEND="wayland")
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen([str(tracer), str(trace), str(binary), str(document)],
                                       env=env, stdout=log, stderr=log, start_new_session=True)
            try:
                deadline = time.monotonic() + 25
                while True:
                    windows = json.loads(subprocess.check_output(["hyprctl", "clients", "-j"], timeout=3))
                    if any(c["title"] == document.name + " — Nameh" for c in windows):
                        break
                    if process.poll() is not None or time.monotonic() > deadline:
                        log.seek(0)
                        raise RuntimeError("traced opening failed: " + log.read().decode(errors="replace"))
                    time.sleep(0.05)
                time.sleep(2)
                subprocess.run(["gdbus", "call", "--session", "--dest", "io.github.arash.mdview",
                    "--object-path", "/io/github/arash/mdview", "--method", "org.gtk.Actions.Activate",
                    "quit", "[]", "{}"], check=True, capture_output=True, timeout=5)
                result = process.wait(timeout=10)
                unchanged = document.read_bytes() == source
                report = {"release_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                          "tracer_controls_pass": True, "trace_exit": result,
                          "source_unchanged": unchanged, "trace": str(trace),
                          "scope": "nameh and traced descendants, Linux x86-64; Unix IPC allowed; no external activation"}
                args.output.write_text(json.dumps(report, indent=2) + "\n")
                print(json.dumps(report), flush=True)
                if result != 0 or not unchanged:
                    raise RuntimeError("network/source validation failed; inspect trace")
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, 15)
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
