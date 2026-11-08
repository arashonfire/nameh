#!/usr/bin/env python3
"""Test desktop assets in a disposable XDG installation on Hyprland.

Requires a release build, desktop-file-utils, shared-mime-info, gio, gtk4-launch,
hyprctl and a graphical session. Does not register MIME defaults or install into
user directories. Tests Wayland and X11; a missing display is reported as unavailable.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "io.github.arash.mdview"


def run(*args, env=None):
    # Launched apps inherit stdout; a pipe would remain open after the launcher exits.
    with tempfile.TemporaryFile(mode="w+") as output:
        subprocess.run(args, env=env, stdout=output, stderr=subprocess.STDOUT,
                       timeout=10, check=True)
        output.seek(0)
        return output.read().strip()


def windows():
    return [c for c in json.loads(run("hyprctl", "clients", "-j")) if c["class"] in [APP_ID, "nameh"]]


def wait_for(predicate):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        found = predicate()
        if found:
            return found
        time.sleep(0.05)
    raise RuntimeError("desktop activation timed out")


def quit_app(env):
    run("gdbus", "call", "--session", "--dest", APP_ID, "--object-path",
        "/io/github/arash/mdview", "--method", "org.gtk.Actions.Activate", "quit", "[]", "{}", env=env)
    wait_for(lambda: not windows())
    wait_for(lambda: subprocess.run(["pgrep", "-x", "nameh"], capture_output=True).returncode != 0)


def main():
    if subprocess.run(["pgrep", "-x", "nameh|mdview"], capture_output=True).returncode == 0:
        raise RuntimeError("close existing nameh processes first")
    defaults = {mime: run("xdg-mime", "query", "default", mime)
                for mime in ["text/markdown", "text/x-markdown"]}
    with tempfile.TemporaryDirectory(prefix="nameh-desktop-") as temporary:
        prefix = Path(temporary)
        data = prefix / "share"
        binary = prefix / "bin/nameh"
        config = prefix / "config"
        config.mkdir()
        gtk_config = Path.home() / ".config/gtk-4.0"
        if gtk_config.exists():
            (config / "gtk-4.0").symlink_to(gtk_config)
        env = dict(os.environ, XDG_DATA_HOME=str(data), XDG_CONFIG_HOME=str(config),
                   PATH=str(binary.parent) + os.pathsep + os.environ["PATH"],
                   CARGO_HOME=os.environ.get("CARGO_HOME", str(ROOT / ".cargo-home")))
        run("make", "-C", str(ROOT), "install", f"PREFIX={prefix}", env=env)
        installed = [binary, data / f"applications/{APP_ID}.desktop",
                     data / f"mime/packages/{APP_ID}.xml",
                     data / "doc/nameh/README.md", data / "doc/nameh/USER_GUIDE.md",
                     data / "licenses/nameh/LICENSE"]
        installed.extend(data / f"icons/hicolor/{size}x{size}/apps/{APP_ID}.png"
                         for size in [16, 24, 32, 48, 64, 128, 256, 512])
        assert all(path.is_file() for path in installed)
        assert binary.read_bytes() == (ROOT / "target/release/nameh").read_bytes()
        desktop = data / f"applications/{APP_ID}.desktop"
        run("desktop-file-validate", str(desktop))
        run("update-mime-database", str(data / "mime"), env=env)
        run("update-desktop-database", str(data / "applications"), env=env)
        print(json.dumps({"desktop_valid": True, "original_defaults": defaults}), flush=True)
        for backend, display in [("wayland", "WAYLAND_DISPLAY"), ("x11", "DISPLAY")]:
            if not env.get(display):
                print(json.dumps({"backend": backend, "unavailable": display}), flush=True)
                continue
            env["GDK_BACKEND"] = backend
            try:
                # Launcher activation without files, followed by the desktop file
                # launch API used by file managers. No default association is needed.
                run("gtk4-launch", APP_ID, env=env)
                wait_for(windows)
                files = [ROOT / "fixtures/basic.md", ROOT / "fixtures/notes/Unicode café.md"]
                run("gio", "launch", str(desktop), *(str(f) for f in files), env=env)
                opened = wait_for(lambda: (w if len(w := windows()) == 2 and
                    {c["title"] for c in w} == {f.name + " — Nameh" for f in files} else None))
                assert all(bool(c["xwayland"]) == (backend == "x11") for c in opened)
                print(json.dumps({"backend": backend, "titles": [c["title"] for c in opened],
                                  "launcher_and_file_open": True}), flush=True)
            finally:
                if windows():
                    quit_app(env)
        unrelated = data / "doc/keep.txt"
        unrelated.write_text("unrelated installation")
        run("make", "-C", str(ROOT), "uninstall", f"PREFIX={prefix}", env=env)
        assert not any(path.exists() for path in installed)
        assert unrelated.read_text() == "unrelated installation"
        print(json.dumps({"make_install_uninstall": True, "unrelated_file_preserved": True}), flush=True)
    assert defaults == {mime: run("xdg-mime", "query", "default", mime) for mime in defaults}
    print(json.dumps({"defaults_unchanged": True, "temporary_install_removed": True}))


if __name__ == "__main__":
    main()
