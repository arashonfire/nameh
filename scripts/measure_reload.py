#!/usr/bin/env python3
"""Profile release reload stability and return-from-5-MB RSS on Hyprland.

Requires a desktop/session bus, hyprctl, gdbus, wtype, a release build, and
no existing mdview instance. Uses disposable source/preferences, default GTK
renderer and compositor geometry (keep mdview focused throughout). Samples
RSS every 10 ms, records transient peaks separately from twenty-second reload
and return-to-small samples plus cache-settled picker samples. Fails if idle RSS reaches
180 decimal MB after settling. Immediate picker samples remain in the report.
--large-kind code also exercises an embedded fenced block.
--output writes JSON lines to a new file. Sampled peaks are not exhaustive.
"""

import argparse
import json
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time


def keys(*arguments):
    subprocess.run(["wtype", *arguments], check=True, timeout=10)


def clients():
    return json.loads(subprocess.check_output(["hyprctl", "clients", "-j"], timeout=3))


def measure(large_kind="prose", output=None):
    root = Path(__file__).resolve().parent.parent
    binary = str(root / "target/release/mdview")
    if subprocess.run(["pgrep", "-x", "mdview"], capture_output=True).returncode == 0:
        raise RuntimeError("close existing mdview instances before measuring")
    with open(binary, "rb") as executable:
        binary_hash = hashlib.file_digest(executable, "sha256").hexdigest()
    if output is not None:
        output.open("x").close()
    with tempfile.TemporaryDirectory(prefix="mdview-step7-") as temporary:
        directory = Path(temporary)
        config = directory / "config"
        config.mkdir()
        gtk_config = Path.home() / ".config/gtk-4.0"
        if gtk_config.exists():
            (config / "gtk-4.0").symlink_to(gtk_config, target_is_directory=True)
        document = directory / "reload café.md"
        original = (root / "fixtures/rendering.md").read_text()
        for name in ["assets", "notes"]:
            (directory / name).symlink_to(root / "fixtures" / name, target_is_directory=True)
        document.write_text(original)
        environment = os.environ.copy()
        environment["XDG_CONFIG_HOME"] = str(config)
        with tempfile.TemporaryFile(mode="w+") as log:
            app = subprocess.Popen(
                [binary, str(document)], env=environment, stdout=log, stderr=log
            )

            def resident_bytes(pid):
                rows = Path(f"/proc/{pid}/status").read_text().splitlines()
                kib = int(next(row for row in rows if row.startswith("VmRSS:")).split()[1])
                return kib * 1024

            def rss():
                value = resident_bytes(app.pid)
                return {
                    "bytes": value,
                    "mib": round(value / 1024**2, 2),
                    "mb": round(value / 1e6, 2),
                }

            def checkpoint_rss():
                # Include children started by any worker thread, not just the
                # main thread's /proc/PID/task/PID/children list.
                pending = [app.pid]
                seen = {app.pid}
                children = []
                while pending:
                    pid = pending.pop()
                    for child_file in Path(f"/proc/{pid}/task").glob("*/children"):
                        try:
                            found = [int(value) for value in child_file.read_text().split()]
                        except OSError:
                            continue
                        for child in found:
                            if child not in seen:
                                seen.add(child)
                                pending.append(child)
                                children.append(child)
                parent_bytes = resident_bytes(app.pid)
                total = parent_bytes
                live_children = []
                for child in sorted(children):
                    try:
                        total += resident_bytes(child)
                        live_children.append(child)
                    except (OSError, StopIteration):
                        pass
                return {
                    "bytes": total,
                    "parent_bytes": parent_bytes,
                    "mib": round(total / 1024**2, 2),
                    "mb": round(total / 1e6, 2),
                    "children": live_children,
                }

            sampled = []
            sampling_done = threading.Event()

            def sample():
                while not sampling_done.is_set():
                    try:
                        sampled.append((time.monotonic(), rss()["mb"]))
                    except (OSError, StopIteration):
                        return
                    sampling_done.wait(0.01)

            sampler = threading.Thread(target=sample, daemon=True)
            sampler.start()

            def peak_since(start):
                values = [value for at, value in sampled if at >= start]
                return max(values, default=0)

            def report(**values):
                line = json.dumps(values)
                print(line, flush=True)
                if output is not None:
                    with output.open("a") as result:
                        result.write(line + "\n")

            def wait_for_title(filename):
                deadline = time.monotonic() + 15
                while not any(
                    c["pid"] == app.pid and c["title"].startswith(filename + " —")
                    for c in clients()
                ):
                    if app.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(f"document did not finish opening: {filename}")
                    time.sleep(0.05)

            def send_keys(*arguments):
                active = json.loads(subprocess.check_output(["hyprctl", "activewindow", "-j"], timeout=3))
                if active.get("pid") != app.pid:
                    raise RuntimeError("keep mdview focused while measuring keyboard input")
                keys(*arguments)

            def picker_cancel():
                send_keys("-M", "ctrl", "-k", "o", "-m", "ctrl")
                time.sleep(0.4)
                report(picker_geometry=[c["size"] for c in clients() if c["pid"] == app.pid and c["title"] == "Open a Markdown file"])
                send_keys("-k", "Escape")
                time.sleep(1)

            try:
                time.sleep(1)
                if app.poll() is not None:
                    raise RuntimeError("mdview exited; close existing instances first")
                assert any(client["pid"] == app.pid for client in clients())
                initial_geometry = [c["size"] for c in clients() if c["pid"] == app.pid]
                report(initial=checkpoint_rss(), initial_geometry=initial_geometry, release_sha256=binary_hash)
                reload_start = time.monotonic()
                samples = []
                for index in range(30):
                    data = original + f"\n\nReload cycle {index}\n"
                    if index % 2:
                        replacement = directory / "replacement"
                        replacement.write_text(data)
                        replacement.replace(document)
                    else:
                        document.write_text(data)
                    time.sleep(0.35)
                    samples.append(checkpoint_rss()["mb"])
                time.sleep(1)
                report(
                    after_30_reloads=checkpoint_rss(),
                    coarse_sampled_peak_mb=max(samples),
                    reload_sampled_peak_mb=peak_since(reload_start),
                    last_10_range_mb=[min(samples[-10:]), max(samples[-10:])],
                )
                time.sleep(19)
                reload_idle_rss = checkpoint_rss()
                report(reload_idle_after_20_seconds=reload_idle_rss)
                picker_cancel()
                normal_rss = checkpoint_rss()
                normal_geometry = [c["size"] for c in clients() if c["pid"] == app.pid]
                report(
                    normal_post_picker_cancel=normal_rss,
                    geometry=normal_geometry,
                )
                # GTK 4.24's GPU cache GC has a 15-second period; allow two
                # periods for a newly unused cached surface, without overriding it.
                time.sleep(30)
                normal_idle_rss = checkpoint_rss()
                report(normal_picker_idle_after_31_seconds=normal_idle_rss)
                stress_start = time.monotonic()
                paragraph = "A responsive document with café, فارسی and 日本語 for reading.\n\n"
                large = directory / "large.md"
                large_source = (
                    "# Large\n\n"
                    + paragraph * (5_000_000 // len(paragraph.encode()) + 1)
                    + "\nlarge-end\n"
                )
                if large_kind == "code":
                    large_source = "# Large\n\n```text\n" + large_source + "\n```\n"
                large.write_text(large_source)
                report(large_kind=large_kind, large_bytes=large.stat().st_size)
                subprocess.run([binary, str(large)], env=environment, check=True, timeout=10)
                time.sleep(0.05)
                send_keys("-M", "ctrl", "-k", "f", "-m", "ctrl", "large-end")
                time.sleep(4)
                wait_for_title(large.name)
                report(large_document_search_rss=checkpoint_rss())
                report(large_title=next(c["title"] for c in clients() if c["pid"] == app.pid))
                send_keys("-k", "Escape", "-M", "ctrl", "-k", "Home", "-m", "ctrl")
                subprocess.run(
                    [binary, str(root / "fixtures/notes/Unicode café.md")],
                    env=environment, check=True, timeout=10,
                )
                wait_for_title("Unicode café.md")
                time.sleep(1)
                report(
                    small_titles=[c["title"] for c in clients() if c["pid"] == app.pid],
                    returned_to_small=checkpoint_rss(),
                )
                time.sleep(20)
                idle_rss = checkpoint_rss()
                report(small_after_20_seconds=idle_rss)
                picker_cancel()
                final_rss = checkpoint_rss()
                final_geometry = [c["size"] for c in clients() if c["pid"] == app.pid]
                geometry_stable = initial_geometry == normal_geometry == final_geometry
                # A picker redraw can briefly populate driver-backed surfaces.
                # Keep the one-second sample above and measure settled idle too.
                report(post_picker_cancel=final_rss)
                time.sleep(30)
                final_idle_rss = checkpoint_rss()
                accepted = all(
                    value["bytes"] < 180_000_000
                    for value in (reload_idle_rss, normal_idle_rss, idle_rss, final_idle_rss)
                )
                report(
                    final_picker_idle_after_31_seconds=final_idle_rss,
                    stress_sampled_peak_mb=peak_since(stress_start),
                    rss_budget_mb=180,
                    idle_budget_pass=accepted,
                    final_geometry=final_geometry,
                    geometry_stable=geometry_stable,
                    sample_count=len(sampled),
                    largest_sample_gap_ms=round(
                        max((b[0] - a[0] for a, b in zip(sampled, sampled[1:])), default=0)
                        * 1000, 2,
                    ),
                )
                subprocess.run(
                    ["gdbus", "call", "--session", "--dest", "io.github.arash.mdview",
                     "--object-path", "/io/github/arash/mdview", "--method",
                     "org.gtk.Actions.Activate", "quit", "[]", "{}"],
                    check=True, capture_output=True, timeout=5,
                )
                report(exit=app.wait(timeout=5))
                log.seek(0)
                print(log.read(), flush=True)
                if not geometry_stable:
                    raise RuntimeError("window geometry changed; repeat the measurement")
                if not accepted:
                    raise RuntimeError(
                        "normal or retained idle RSS exceeds the unchanged 180 MB budget"
                    )
            finally:
                sampling_done.set()
                sampler.join(timeout=2)
                if app.poll() is None:
                    app.terminate()
                    app.wait(timeout=5)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--large-kind", choices=("prose", "code"), default="prose")
    parser.add_argument("--output", type=Path, help="write JSON lines to a new results file")
    arguments = parser.parse_args()
    measure(arguments.large_kind, arguments.output)
