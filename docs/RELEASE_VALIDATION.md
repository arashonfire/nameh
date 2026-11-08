# Nameh 0.1.0 release validation

These results were recorded under the working name `mdview`. The app is now named
Nameh, and the current executable is `nameh`. The old measured executable hashes,
paths, screenshots and raw evidence retain their historical identifiers; they are
not fresh measurements of the renamed binary. Stable application/asset IDs and
preferences retain their existing paths. The user has since initialized Git with commit
`5a3625d` (`first commit`); earlier validation predates the repository.

Step 9 is complete with the verification limits below. The user explicitly accepted
the measured fresh-process startup results with OS caches warm/uncontrolled.
Native build/install/uninstall tooling and the user-selected MIT license are
present. No release archive has been published and no persistent
user or system installation has been made. GTK's default renderer and the
180,000,000-byte settled typical-document RSS budget are unchanged.

## Reproduce

From the repository root:

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo fmt --check
cargo check --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline -- --nocapture
cargo build --release --locked --offline
cargo build --release --locked --offline --target-dir /tmp/nameh-clean-build
python scripts/check_desktop.py
python scripts/check_network.py --output /tmp/nameh-network.json
python scripts/measure_release.py --output /tmp/nameh-startup.jsonl
python scripts/measure_reload.py --output /tmp/nameh-prose.jsonl
python scripts/measure_reload.py --large-kind code --output /tmp/nameh-code.jsonl
```

Use new output paths. Run graphical checks sequentially with no existing Nameh process;
the network check requires Linux x86-64, a C compiler and permission to trace its
own children. The startup script requires Hyprland 0.57's Lua dispatcher, grim,
gdbus and wtype. It uses an empty temporary workspace, restores the original
workspace/monitor, and writes five screenshots beside its JSONL output. Inspect
all five for readable content. Run reload measurements on an empty workspace
with stable geometry, keeping Nameh focused; they take about two minutes each.
No benchmark changes GTK's renderer, GPU cache, desktop configuration or MIME defaults.

## Build and functional checklist

| Check | Result and evidence |
| --- | --- |
| Formatting, check, strict all-target Clippy | Pass, locked/offline |
| Full test suite | All 16 pass with desktop/session-bus access; sandbox-only GTK launch fails because the display is inaccessible |
| Release build with an empty target directory | Pass, locked/offline; byte-identical to the measured binary |
| CLI, picker, multiple windows, Unicode paths | GTK regression and actual Wayland/Xwayland desktop-launch tests pass |
| Rendering, read-only copy, embedded scrolling | GTK regression passes; 100 KB startup captures inspected for rendered Unicode text |
| Search, keyboard controls, zoom, appearance/preferences | GTK regression passes; release zoom/reset and three picker cycles also exercised |
| Internal navigation, rejected targets, launch-error recovery | Parser/navigation/GTK regressions pass; external dispatch uses a recording launcher |
| Ordinary/atomic reload, deletion/recreation, Retry, stale results | Real GIO regression passes |
| 5 MB prose/search and fenced-code responsiveness | Full regression main-loop gaps 16.92 / 21.60 ms, below 250 ms |
| Keyboard, AT-SPI, contrast, reduced motion | Step 8 evidence retained; current full GTK contrast regression passes at 13.77:1 |
| Install, native launch, uninstall | Actual Makefile in a disposable prefix passes on Wayland and Xwayland; binary/assets/guide/license installed, unrelated file preserved |
| Source integrity | Byte comparisons in navigation/image regressions; full fixture hashes and temporary 100 KB source unchanged after release exercise; network fixture unchanged |
| No unsolicited Internet activity while opening | Traced Linux x86-64 process and descendants use Unix sockets only; local HTTP sentinel records zero requests |

Known normal accessibility-bus refusal and occasional discarded/duplicate frame
warnings do not fail the tests. Actual speech, physical pointer selection/drop,
file-manager GUI gestures and browser/mail/other-file handlers remain unverified.
The API/signal/recording-launcher checks do not establish those manual interactions.

## Startup and typical-document memory

Reference: Arch Linux x86-64, Intel Core Ultra 9 275HX, 32,264,084 KiB physical
memory, Hyprland 0.57.1 Wayland, GTK 4.24.1, Rust 1.99.1, existing GTK styling and
default renderer. Release executable: 2,842,352 bytes, SHA-256
`78a674f87e5d886c44e62b123528767901ccd19a204f346faa002637719c8877`.
An independent build in an initially empty target directory has the same hash.

Five fresh processes each opened a temporary **100,000-byte** UTF-8 document
containing Unicode prose, a blocked remote image and an inactive web link, with
isolated preferences on an empty workspace. Window allocation was **1520 × 956**
logical pixels. OS filesystem caches were warm/uncontrolled; no cache eviction or
reboot was performed. Here cold application launch means a fresh application
process with no existing Nameh instance; cold OS-cache performance is unmeasured.

The clock starts before process creation. The script waits until the compositor
reports a mapped window with the loaded document's title, allows 40 ms for frames,
then captures the window. Reported capture completion includes screenshot encoding
and IPC overhead. All five captures were inspected: the heading, blocked-image
description, link and Unicode prose are rendered. This supplies an observed upper
bound to captured readable content for these runs, rather than treating mapping
as sufficient evidence. Desktop transparency/wallpaper remains visible.

| Run | Loaded title mapped (ms) | Capture complete (ms) | RSS after capture + 1 s, MiB / MB |
| --- | ---: | ---: | ---: |
| 1 | 342.74 | 896.04 | 149.055 / 156.295 |
| 2 | 280.44 | 867.03 | 148.707 / 155.931 |
| 3 | 288.95 | 827.00 | 149.312 / 156.566 |
| 4 | 286.12 | 838.49 | 149.066 / 156.307 |
| 5 | 304.00 | 865.09 | 148.801 / 156.029 |

Median capture completion: **865.09 ms**; all measured runs are below one second.
RSS sums `/proc/PID/status` recursively through children of every thread. No child
process remained at any checkpoint. MB means 1,000,000 bytes; MiB means 1,048,576.

After five zoom-in operations to 150%, reset to 100%, floating/resizing through
850 × 650, 1100 × 800 and back, and three picker-open/cancel cycles, the fixed
31-second idle sample is **156.219 MiB / 163.807 MB**. The sampled process-tree peak
during those operations is **165.356 MB**, at roughly 20 ms sampling. This is an
observed sample maximum, not an exhaustive peak bound or pathological-input limit.

Raw final evidence: [isolated run](step9-release-isolated.jsonl) and captures
`step9-release-isolated-1.png` through `step9-release-isolated-5.png`.
The earlier `step9-release.jsonl` is a diagnostic run contaminated by concurrent
compilation and interrupted by obsolete Hyprland dispatcher syntax; it is excluded
from acceptance. `step9-release-final.jsonl` used a shared workspace with a smaller
allocation and is also excluded from reference results. Their screenshots were
removed. No user source or persistent preferences were changed.

## Reload and return from 5 MB

The final release binary was remeasured with `measure_reload.py` for both prose
and fenced code on an empty workspace at **1520 × 956**, original **720 × 480**
picker, isolated documents/preferences, default renderer and unchanged fixed idle
intervals. Thirty alternating ordinary/atomic saves precede large-document use,
real Find/Escape/Ctrl+Home, return to a small Unicode document and picker cancel.
The prior workspace/monitor was restored after both runs.

Values are **MiB / decimal MB**, recursively including children of every thread:

| Checkpoint | Prose, 5,000,082 bytes | Fenced code, 5,000,104 bytes |
| --- | ---: | ---: |
| Initial, one second | 167.32 / 175.45 | 167.19 / 175.31 |
| Reload, 20 seconds idle | 158.64 / 166.35 | 160.27 / 168.05 |
| Normal picker, 31 seconds idle | 170.43 / 178.71 | 170.44 / 178.72 |
| Large document with search | 187.15 / 196.24 | 208.62 / 218.76 |
| Return to small file, 20 seconds | 162.05 / 169.93 | 166.92 / 175.03 |
| Final picker, 31 seconds idle | 158.54 / 166.24 | 164.54 / 172.54 |

Both fixed idle-budget checks pass, geometry is stable and both exits are 0. No
children remain at any checkpoint. Observed main-process reload peaks are
180.07 / 179.92 MB; large-use peaks are **226.59 / 227.70 MB**. These are sampled
peaks, not hard caps. The 180 MB criterion applies to settled typical-document
use; the 5 MB criterion is responsiveness. Approximately 10 ms peak sampling
targets the main process; checkpoint totals account for the full process tree.
Largest sampling gaps: 14.17 / 13.39 ms. Evidence:
[prose](step9-memory-prose.jsonl), [code](step9-memory-code.jsonl).

## Runtime network check

`check_network.py` builds `trace_network.c` into a temporary directory. Before
opening Nameh it verifies that an AF_UNIX control passes and an AF_INET socket
creation control fails. The helper uses Linux ptrace syscall-entry records and
follows clone/fork/vfork/exec descendants, including worker threads. It rejects
socket families other than AF_UNIX, non-Unix connect/sendto addresses, unsupported
syscall architectures and uninspected io_uring setup. It does not inject a sandbox
or silently block network calls; attempted Internet socket creation fails the check.

The release process opens a copy of rendering.md with local images, blocked HTTP
and HTTPS images, raw HTML and unactivated web/mail links, then exits cleanly.
Only AF_UNIX creation and destinations are observed, with zero rejected or
uninspected operations and unchanged input bytes. The command inherits no Internet
sockets from the Python launcher. Evidence: [result](step9-network.json) and
[syscall trace](step9-network.log). This covers the tested process tree and opening
path; it does not trace unrelated desktop daemons or actual external handlers.
The separate 100 KB run's local HTTP sentinel also records zero requests.

## Outstanding verification

- Controlled cold-filesystem-cache startup remains unperformed. **The user
  explicitly accepted the measured fresh-process results for the startup
  criterion**, with warm/uncontrolled OS caches disclosed. No cold OS-cache
  performance claim is made.
- Orca is unavailable. Speech/announcement timing and full screen-reader usability
  remain unverified; current AT-SPI text/roles and keyboard evidence remain valid.
- Physical pointer/file-manager gestures, actual external handlers, standalone
  Xorg, other libc targets and the full range of GTK 4.10 distributions remain
  unverified. Wayland and Xwayland pass on the reference desktop.
- Pathological unbroken lines, enormous link labels, extreme nesting and heavily
  image/table-based 5 MB files lack comprehensive performance profiling. Existing
  5 MB prose/code, decoder bounds and settled-memory evidence have narrower scope.

The [user guide](USER_GUIDE.md) covers opening, reading, search, copying, links,
appearance, preferences and recovery. README provides native install/uninstall and
DESTDIR staging instructions. MIT was selected explicitly by the user during this
session; no license choice remains outstanding.
