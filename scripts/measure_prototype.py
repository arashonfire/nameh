#!/usr/bin/env python3
"""Measure fresh prototype processes on a Hyprland reference desktop.

Requires a release build, hyprctl, gdbus, /proc, and no existing mdview process.
Reports mapped-window latency with warm filesystem caches, not cold readable time.
"""

import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def measure():
    results = []
    root = Path(__file__).resolve().parent.parent
    for index in range(5):
        with tempfile.TemporaryFile() as log:
            start = time.perf_counter()
            app = subprocess.Popen(
                [str(root / "target/release/mdview"), "--prototype"],
                stdout=log,
                stderr=log,
            )
            try:
                while time.perf_counter() - start < 10:
                    if app.poll() is not None:
                        raise RuntimeError("mdview exited; close any existing instance first")
                    clients = json.loads(
                        subprocess.check_output(["hyprctl", "clients", "-j"], timeout=2)
                    )
                    if any(
                        client["pid"] == app.pid and client.get("mapped", True)
                        for client in clients
                    ):
                        elapsed = time.perf_counter() - start
                        break
                    time.sleep(0.01)
                else:
                    raise RuntimeError("no mapped prototype window within 10 seconds")
                time.sleep(1)
                status = Path(f"/proc/{app.pid}/status").read_text().splitlines()
                rss_kib = int(next(row for row in status if row.startswith("VmRSS:")).split()[1])
                children = Path(f"/proc/{app.pid}/task/{app.pid}/children").read_text().strip()
                if children:
                    raise RuntimeError(f"child process memory requires accounting: {children}")
                result = {
                    "run": index + 1,
                    "mapped_ms": round(elapsed * 1000, 1),
                    "rss_mib": round(rss_kib / 1024, 2),
                }
                results.append(result)
                print(json.dumps(result), flush=True)
                subprocess.run(
                    ["gdbus", "call", "--session", "--dest", "io.github.arash.mdview",
                     "--object-path", "/io/github/arash/mdview", "--method",
                     "org.gtk.Actions.Activate", "quit", "[]", "{}"],
                    check=True, capture_output=True, timeout=3,
                )
                if app.wait(timeout=5) != 0:
                    raise RuntimeError("mdview did not exit cleanly")
            finally:
                if app.poll() is None:
                    app.terminate()
                    app.wait(timeout=5)
    print(json.dumps({
        "median_mapped_ms": statistics.median(row["mapped_ms"] for row in results),
        "median_rss_mib": statistics.median(row["rss_mib"] for row in results),
    }))


if __name__ == "__main__":
    measure()
