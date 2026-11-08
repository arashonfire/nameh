#!/usr/bin/env python3
"""Measure fresh release processes on Hyprland with a 100,000-byte document.

Requires Hyprland 0.57 Lua dispatch, grim, gdbus, wtype and no mdview instance. Saves screenshots
for human readable-content review. Caches are NOT dropped. Capture completion
includes two-frame allowance and screenshot overhead; mapping alone is not used
as readable latency. Recursively sums RSS from descendants of every thread.
Uses an empty temporary workspace and restores the original workspace afterward.
Only a local HTTP sentinel is checked; use check_network.py for syscall tracing.
Use --output NEW_FILE; screenshots go beside that file. Exit nonzero for failed
fixed idle-memory checkpoints, capture latency >=1 s, source changes or requests.
"""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "io.github.arash.mdview"


def run(*args, **kwargs):
    return subprocess.check_output(args, timeout=10, **kwargs)


def clients():
    return json.loads(run("hyprctl", "clients", "-j"))


def resident_tree(pid):
    pending, seen, values = [pid], set(), {}
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            rows = Path(f"/proc/{current}/status").read_text().splitlines()
            values[current] = int(next(r for r in rows if r.startswith("VmRSS:")).split()[1]) * 1024
            for child_file in Path(f"/proc/{current}/task").glob("*/children"):
                try:
                    pending.extend(map(int, child_file.read_text().split()))
                except OSError:
                    pass
        except (OSError, StopIteration):
            pass
    total = sum(values.values())
    return {"bytes": total, "mb": round(total / 1e6, 3),
            "mib": round(total / 1024**2, 3), "process_bytes": values}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if subprocess.run(["pgrep", "-x", "mdview"], capture_output=True).returncode == 0:
        raise RuntimeError("close all mdview instances first")
    if os.environ.get("GSK_RENDERER"):
        raise RuntimeError("unset GSK_RENDERER for default-renderer acceptance")
    output = args.output.resolve()
    output.open("x").close()

    def report(**record):
        line = json.dumps(record, ensure_ascii=False)
        print(line, flush=True)
        with output.open("a") as stream:
            stream.write(line + "\n")

    requests = []

    class Sentinel(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(self.path)
            self.send_response(404)
            self.end_headers()

        def log_message(self, *unused):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Sentinel)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    binary = ROOT / "target/release/mdview"
    fixture_hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in (ROOT / "fixtures").rglob("*") if p.is_file()}
    report(release_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
           cache_policy="fresh processes; warm/uncontrolled OS caches; no cache eviction",
           os=Path("/etc/os-release").read_text(), machine=os.uname().machine,
           cpu=next(x for x in Path("/proc/cpuinfo").read_text().splitlines() if x.startswith("model name")),
           memory=Path("/proc/meminfo").read_text().splitlines()[0],
           gtk=run("pkg-config", "--modversion", "gtk4").decode().strip(),
           rust=run("rustc", "--version").decode().strip(),
           display="Hyprland Wayland", network_check="local sentinel only; separate check_network.py traces syscalls")
    original_workspace = json.loads(run("hyprctl", "activeworkspace", "-j"))
    validation_workspace = "name:mdview-release-" + str(os.getpid())
    results = []
    try:
        run("hyprctl", "eval", "hl.dispatch(hl.dsp.focus({workspace=" + json.dumps(validation_workspace) +
            ", on_current_monitor=true}))")
        time.sleep(0.3)
        with tempfile.TemporaryDirectory(prefix="mdview-release-") as temporary:
            directory = Path(temporary)
            source = directory / "100 KB café.md"
            paragraph = "Readable café, Ελληνικά, فارسی and 日本語. A local document for release validation.\n\n"
            header = ("# Release validation\n\n"
                      f"![Remote image blocked](http://127.0.0.1:{server.server_port}/blocked.png)\n\n"
                      f"[Explicit activation only](http://127.0.0.1:{server.server_port}/link)\n\n")
            data = (header + paragraph * 1200).encode()[:99_990].decode("utf-8", errors="ignore").encode()
            data += b" " * (100_000 - len(data) - 1) + b"\n"
            source.write_bytes(data)
            for index in range(5):
                config = directory / f"config-{index}"
                config.mkdir()
                gtk_config = Path.home() / ".config/gtk-4.0"
                if gtk_config.exists():
                    (config / "gtk-4.0").symlink_to(gtk_config)
                env = dict(os.environ, XDG_CONFIG_HOME=str(config), GDK_BACKEND="wayland")
                samples, done = [], threading.Event()
                with tempfile.TemporaryFile() as log:
                    start = time.monotonic()
                    app = subprocess.Popen([str(binary), str(source)], env=env, stdout=log, stderr=log)

                    def sample():
                        while not done.is_set():
                            samples.append(resident_tree(app.pid)["bytes"])
                            done.wait(0.02)

                    sampler = threading.Thread(target=sample, daemon=True)
                    sampler.start()
                    try:
                        deadline = start + 15
                        while True:
                            window = next((c for c in clients() if c["pid"] == app.pid
                                           and c.get("mapped") and c["title"] == source.name + " — mdview"), None)
                            if window:
                                break
                            if app.poll() is not None or time.monotonic() > deadline:
                                raise RuntimeError("document presentation did not complete")
                            time.sleep(0.01)
                        mapped_ms = (time.monotonic() - start) * 1000
                        # Allow two nominal 60 Hz frames before compositor capture.
                        time.sleep(0.04)
                        active = json.loads(run("hyprctl", "activewindow", "-j"))
                        if active.get("pid") != app.pid:
                            raise RuntimeError("mdview lost focus; stopped screen capture")
                        x, y = window["at"]
                        width, height = window["size"]
                        screenshot = output.with_name(output.stem + f"-{index + 1}.png")
                        run("grim", "-g", f"{x},{y} {width}x{height}", str(screenshot))
                        capture_ms = (time.monotonic() - start) * 1000
                        time.sleep(1)
                        idle = resident_tree(app.pid)
                        result = {"run": index + 1, "source_bytes": len(data), "mapped_content_installed_ms": round(mapped_ms, 2),
                                  "capture_complete_ms": round(capture_ms, 2), "screenshot": str(screenshot),
                                  "geometry": window["size"], "idle_after_capture_plus_1s": idle}
                        results.append(result)
                        report(**result)
                        if index == 4:
                            def key(*arguments):
                                active = json.loads(run("hyprctl", "activewindow", "-j"))
                                if active.get("pid") != app.pid:
                                    raise RuntimeError("mdview lost focus; stopped keyboard input")
                                run("wtype", *arguments)

                            stress_start = len(samples)
                            for _ in range(5):
                                key("-M", "ctrl", "-k", "plus", "-m", "ctrl")
                                time.sleep(0.15)
                            key("-M", "ctrl", "-k", "0", "-m", "ctrl")
                            address = "address:" + window["address"]
                            # Hyprland 0.57 uses typed Lua dispatchers.
                            if not window["floating"]:
                                run("hyprctl", "eval", "hl.dispatch(hl.dsp.window.float({window=" + json.dumps(address) + "}))")
                            for width, height in [(850, 650), (1100, 800), (850, 650)]:
                                run("hyprctl", "eval", "hl.dispatch(hl.dsp.window.resize({window=" + json.dumps(address) +
                                    f", x={width}, y={height}, relative=false" + "}))")
                                time.sleep(0.2)
                            for _ in range(3):
                                key("-M", "ctrl", "-k", "o", "-m", "ctrl")
                                time.sleep(0.4)
                                key("-k", "Escape")
                                time.sleep(0.4)
                            time.sleep(31)
                            settled = resident_tree(app.pid)
                            report(zoom_resize_three_picker_cycles={"sampled_tree_peak_bytes": max(samples[stress_start:]),
                                   "idle_after_31_seconds": settled,
                                   "final_geometry": next(c["size"] for c in clients() if c["pid"] == app.pid)})
                            if settled["bytes"] >= 180_000_000:
                                raise RuntimeError("zoom/resize/picker settled RSS exceeds 180 MB")
                        run("gdbus", "call", "--session", "--dest", APP_ID, "--object-path",
                            "/io/github/arash/mdview", "--method", "org.gtk.Actions.Activate", "quit", "[]", "{}")
                        if app.wait(timeout=5) != 0:
                            raise RuntimeError("unclean exit")
                    finally:
                        done.set()
                        sampler.join(timeout=2)
                        if app.poll() is None:
                            app.terminate()
                            app.wait(timeout=5)
                if source.read_bytes() != data:
                    raise RuntimeError("source changed")
        unchanged = all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == value for p, value in fixture_hashes.items())
        accepted = unchanged and not requests and all(r["capture_complete_ms"] < 1000 and
                    0 < r["idle_after_capture_plus_1s"]["bytes"] < 180_000_000 for r in results)
        report(median_capture_ms=statistics.median(r["capture_complete_ms"] for r in results),
               sources_unchanged=unchanged, sentinel_requests=requests,
               measured_thresholds_pass=accepted, screenshot_review="required; inspect all five captures",
               cold_filesystem_cache_validation="unperformed", network_trace="run check_network.py separately")
        if not accepted:
            raise RuntimeError("release measurement thresholds failed")
    finally:
        server.shutdown()
        server.server_close()
        run("hyprctl", "eval", "hl.dispatch(hl.dsp.focus({monitor=" + json.dumps(original_workspace["monitor"]) + "}))")
        run("hyprctl", "eval", "hl.dispatch(hl.dsp.focus({workspace=" + str(original_workspace["id"]) +
            ", on_current_monitor=true}))")


if __name__ == "__main__":
    main()
