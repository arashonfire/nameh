#!/usr/bin/env python3
"""Inspect AT-SPI and exercise real reading keys on Hyprland, without Orca.

Requires a release build, PyGObject/Atspi, dbus-daemon, at-spi2-registryd,
hyprctl and wtype. Uses temporary accessibility bus/config; never changes desktop
services. Stops keyboard input if nameh loses focus. This does not test speech.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    if subprocess.run(["pgrep", "-x", "nameh|mdview"], capture_output=True).returncode == 0:
        raise RuntimeError("close existing nameh instances first")
    with tempfile.TemporaryDirectory(prefix="nameh-accessibility-") as temporary:
        config = Path(temporary)
        gtk_config = Path.home() / ".config/gtk-4.0"
        if gtk_config.exists():
            (config / "gtk-4.0").symlink_to(gtk_config)
        bus = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address"],
                               stdout=subprocess.PIPE, text=True)
        registry = app = None
        try:
            address = bus.stdout.readline().strip()
            os.environ["AT_SPI_BUS_ADDRESS"] = address
            import gi
            gi.require_version("Atspi", "2.0")
            from gi.repository import Atspi
            environment = dict(os.environ, XDG_CONFIG_HOME=temporary)
            registry = subprocess.Popen(["/usr/lib/at-spi2-registryd"],
                env=dict(environment, DBUS_SESSION_BUS_ADDRESS=address),
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                registered = subprocess.check_output(["gdbus", "call", "--address", address,
                    "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
                    "--method", "org.freedesktop.DBus.NameHasOwner", "org.a11y.atspi.Registry"], text=True)
                if "true" in registered:
                    break
                time.sleep(0.05)
            app = subprocess.Popen([str(ROOT / "target/release/nameh"),
                                    str(ROOT / "fixtures/rendering.md")], env=environment,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            def records():
                result = []
                def walk(node, depth=0):
                    if depth > 16:
                        return
                    try:
                        name, role = node.get_name(), node.get_role_name()
                        text = ""
                        interface = node.get_text_iface()
                        if interface is not None:
                            text = Atspi.Text.get_text(interface, 0, -1)
                        result.append({"role": role, "name": name, "text": text,
                            "focused": node.get_state_set().contains(Atspi.StateType.FOCUSED)})
                        for index in range(node.get_child_count()):
                            walk(node.get_child_at_index(index), depth + 1)
                    except Exception:
                        pass  # Non-text widgets and objects retiring during a refresh.
                desktop = Atspi.get_desktop(0)
                for index in range(desktop.get_child_count()):
                    child = desktop.get_child_at_index(index)
                    if child.get_process_id() == app.pid:
                        walk(child)
                return result

            def wait_for(predicate):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    value = predicate()
                    if value:
                        return value
                    time.sleep(0.05)
                raise RuntimeError("accessibility check timed out")

            def keys(*arguments):
                active = json.loads(subprocess.check_output(["hyprctl", "activewindow", "-j"]))
                if active.get("pid") != app.pid:
                    raise RuntimeError("nameh lost focus; stopped keyboard input")
                subprocess.run(["wtype", *arguments], check=True, timeout=5)

            def loaded_tree():
                current = records()
                return current if any(x["name"] == "Code block: text" for x in current) else None
            tree = wait_for(loaded_tree)
            def text(name):
                return next(x["text"] for x in tree if x["name"] == name)
            assert "Café — Ελληνικά — فارسی — 日本語" in text("Markdown document")
            assert any(x["name"] == "Markdown table" for x in tree)
            assert any("Bold and italic" in (x["text"] or x["name"] or "") for x in tree)
            assert text("Code block: text").startswith("    Indentation")
            assert any(x["role"] == "image" and x["name"] == "A small colored square" for x in tree)
            assert [x["name"] for x in tree if x["role"] == "link"] == ["Basic fixture", "Unicode path", "Long code"]
            keys("-M", "ctrl", "-k", "f", "-m", "ctrl")
            keys("Café")
            wait_for(lambda: any(x["text"] == "1 of 1" for x in records()))
            keys("-M", "ctrl", "-k", "g", "-m", "ctrl")
            keys("-k", "Escape")
            keys("-M", "ctrl", "-k", "o", "-m", "ctrl")
            wait_for(lambda: any(x["name"] == "File or folder path" for x in records()))
            keys("-k", "Escape")
            keys("-M", "ctrl", "-k", "plus", "-m", "ctrl")
            wait_for(lambda: any(x["text"] == "110%" for x in records()))
            keys("-M", "ctrl", "-k", "0", "-m", "ctrl")
            keys("-k", "Tab")
            for _ in range(30):
                focused = [x for x in records() if x["focused"]]
                if any(x["role"] == "link" and x["name"] == "Long code" for x in focused):
                    break
                keys("-k", "Tab")
            else:
                raise RuntimeError("keyboard traversal did not reach the heading link")
            keys("-k", "Return")
            print(json.dumps({"atspi_tree": tree, "keyboard_heading_activation": True, "unicode_table_code_image_links": True,
                "real_keys_find_next_escape_picker_zoom": True,
                "speech_reader": "unavailable; Orca is not installed"}, ensure_ascii=False, indent=2))
        finally:
            for process in [app, registry, bus]:
                if process is not None and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
