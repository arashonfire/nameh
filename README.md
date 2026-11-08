# Nameh

Nameh is a lightweight native Markdown viewer for Linux, built with Rust, GTK4, and pulldown-cmark.

The executable and Cargo package are named `nameh`; the desktop launcher and
window titles use **Nameh**. The app was developed under the working name `mdview`.
Its stable application ID (`io.github.arash.mdview`), desktop/icon/MIME asset IDs
and existing preference path (`mdview/preferences.ini`) are retained for continuity.

## Current status

Steps 1–9 are complete, including accessibility, desktop integration, automatic reload, cooperative large-document loading/search, release validation and native installation tooling. The unchanged 180 MB idle RSS budget passes on the reference GNU/Linux desktop. Open files through the picker, drag and drop, or `nameh PATH`. See [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) for the latest session record and [release validation](docs/RELEASE_VALIDATION.md) for results and unperformed checks. The user selected the [MIT license](LICENSE) and accepted the measured fresh-process startup results with OS caches warm/uncontrolled. No release has been published or persistently installed by the checks.

The project now has a Git repository, initialized by the user with commit
`5a3625d` (`first commit`). Earlier session records describe the environment before
Git was initialized. Use `git status`, `git diff`, and the available history when
reviewing subsequent changes.

## Prerequisites

- Rust and Cargo with Rust 2024 edition support, plus rustfmt and Clippy.
- GTK4 development libraries version 4.10 or newer and their dependencies.
- A C compiler, linker, and pkg-config.
- A running graphical Linux session for launching the application.
- Network access for the initial Cargo dependency download.

On Arch Linux, the build prerequisites are `rust`, `gtk4`, `pkgconf`, and `base-devel`. They were already installed on the development machine.

## Build and run

The local Cargo home keeps dependency downloads inside the writable workspace. Use this environment variable consistently for the commands below; a normal unrestricted development environment may use its default Cargo home instead.

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo build --locked
cargo run --locked
```

Select Open File or press Ctrl+O, or drag one file onto a document window. The picker starts in the current document's directory (the working directory for an empty window). Select a filename and Open, activate a row to open a file or folder, or enter an absolute or relative filesystem path. Up goes to the parent folder; All files expands the Markdown filter. Ctrl+L focuses the path field, Enter opens it, and Escape or Cancel closes the picker.

Each window holds one document. A successful open replaces that window's document; a failed open keeps it visible and shows an error with Retry. Use New Window in the header or Ctrl+N for another document window, Ctrl+W to close a window, and Ctrl+Q to quit. A command sent to an existing instance replaces its active document; extra command-line files open in separate windows.

```sh
cargo run --locked --offline -- "fixtures/notes/Unicode café.md"
cargo run --locked --offline -- fixtures/rendering.md
```

Markdown links (`.md`, `.markdown`, `.mdown`, `.mkd`, case-insensitive) open inside the current window and honor heading fragments. Other readable local files use the desktop's default application after activation. HTTP, HTTPS and mailto links also require explicit activation. Executable files, scripts, desktop shortcuts, unknown URI schemes and remote document sources are blocked. Navigation errors leave the current document available with Retry. A missing fragment in an otherwise readable Markdown file opens the destination and reports the missing heading.

```sh
cargo fmt --check
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings
```

For an optimized build, use `cargo build --release --locked`.

## Local installation and removal

Build and install with the installer script:

```sh
./install.sh
```

This builds the locked release (downloading Cargo dependencies if needed) and
installs into `~/.local`. It adds the application-menu shortcut, icon, and Markdown
MIME definitions and refreshes available desktop caches. It uses this checkout's
`.cargo-home` cache when present, unless you set `CARGO_HOME` yourself. Build
prerequisites and cache tools are listed below; the script does not install them.
Use `./install.sh --offline` when dependencies are already cached.

Remove it with:

```sh
./uninstall.sh
```

Both scripts accept `--prefix /absolute/path` (or `PREFIX`) and honor `DESTDIR`.
Use the same prefix when uninstalling. The uninstaller requires `make`, but does
not require Rust or rebuild the app. Run either script with `--help` for details.

Install build prerequisites above plus `make`. For desktop/MIME cache updates,
install `desktop-file-utils` and `shared-mime-info`; `gtk-update-icon-cache` is
optional. Fetch locked dependencies once, then build and install offline:

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo fetch --locked
make build
make install PREFIX="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
nameh "fixtures/notes/Unicode café.md"
```

Keep `~/.local/bin` on the desktop session's PATH for the launcher (`Exec=nameh %F`).
The usual desktop data directory is `~/.local/share`; a custom prefix also needs
`PREFIX/share` in `XDG_DATA_DIRS`. Nonstandard `XDG_DATA_HOME` layouts need matching
desktop configuration. The binary links to the system GTK/GLib libraries; install
on a compatible Linux system with GTK 4.10 or newer. A binary built here is not a
portable bundle or proof of compatibility with every GTK 4.10 distribution.

Installed files are the binary, desktop entry, scalable icon, MIME declaration,
README, [user guide](docs/USER_GUIDE.md), and MIT license. Cache tools run when
available; no default application is selected. Choose **Nameh** through your
file manager's **Open With** controls if desired.

Remove the installation with the same prefix:

```sh
make uninstall PREFIX="$HOME/.local"
```

Uninstall removes only Nameh's installed files and refreshes available caches.
Documents, preferences, other applications and MIME default selections are
preserved. Empty directories may remain. Preferences can be removed separately
from the path documented under Reading controls.

Stage a native filesystem layout without installing to system or user directories:

```sh
make install DESTDIR=/tmp/nameh-stage PREFIX=/usr
make uninstall DESTDIR=/tmp/nameh-stage PREFIX=/usr
```

`DESTDIR` suppresses cache updates. A package manager should refresh caches after
installation/removal on the destination system. `CARGO_FLAGS` defaults to
`--locked --offline`; use `make build CARGO_FLAGS=--locked` when dependencies have
not yet been fetched. No archive, registry publication or system installation is
performed by the release checks.

To view the bundled rendering prototype, close existing Nameh windows and run:

```sh
cargo run --locked --offline -- --prototype
```

The Open action is disabled in preview mode. The renderer supports headings, paragraphs, nested lists, ordered starts, task markers, quotes, rules, emphasis, strikethrough, inline and fenced/indented code, formatted aligned tables, images, and links. Text sits in a centered column capped at 900 logical pixels. Tables and code retain their own scrollers. Heading links work, including percent-encoded Unicode fragments and links inside tables. Other links use the window's safe navigation pipeline; the bundled preview keeps them inert.

Copying a main-document selection across embedded blocks includes their whole contents; selecting within a block copies its own text. Tables become aligned plain text, code keeps all whitespace, images become descriptions, and links include their classified targets. Continuous selection from partway inside an embedded block into prose remains outside the chosen widget structure. See [rendering results](docs/RENDERING_PROTOTYPE.md) for screenshots, selection boundaries, accessibility checks, and measurements.

Local images resolve against the supplied document directory using filesystem APIs after one percent decode. Loading and decoding use a weak-reference queue with one active GIO worker and the raster-only `image` 0.25.10 library. Files are limited to 8 MiB, 16 million pixels and a 16,384-pixel dimension. Retained textures fit within 1024 × 768 pixels. PNG, JPEG, static GIF, BMP, and PNM are accepted by content signature; animated GIF, SVG, remote sources, unknown formats, missing files, and malformed images get placeholders. The forced raster format also blocks indirect remote image references. It replaces the platform decoder process; no application image-decoder helper is spawned. Pixel, encoded-file and decoder-allocation limits are applied before retaining native GTK textures. The preview's image is read from the repository's fixtures directory, so keep those assets available when running this development preview.

## Automatic reload and large documents

GIO directory notifications reload ordinary writes and rename-over atomic saves after 150 ms of quiet. Monitoring follows both the logical entry and a symlink target, including relative paths containing `..`. Unrelated sibling changes are ignored. Deletion, rename away, unreadable/invalid UTF-8 replacements and failed reloads keep the last readable document and offer Retry. Recreating the watched path recovers automatically; a renamed file is not silently followed to its new name. A filesystem identity/version recheck closes the gap between reading and installing a monitor.

A replacement is prepared while the old document remains available. New requests and newer saves cancel stale presentation futures; request generations still reject stale worker results. Successful reloads restore the viewport using the nearest surviving heading and its relative line offset, or a proportional position if that heading disappears. An open search query is recomputed without scrolling away from the restored viewport. Monitors, queued work and search work are cancelled when navigating or closing. There is no idle polling timer; debounce, layout and restoration sources are temporary.

Rendering inserts UTF-8-safe chunks and yields between work slices, including code blocks, lists and tables. Native read-only text buffers disable undo history. No document is truncated or presented as source text as a large-file fallback. Regression fixtures of at least 5,000,000 bytes cover prose and fenced code, controls during loading, full search counts and revealing the final match. This does not establish responsiveness for every pathological single-line input or image/table-heavy 5 MB document. Large-view retirement and picker close return freed glibc heap pages on a worker after a coalesced one-shot callback; other libc targets use a no-op. There is no permanent polling timer or renderer override. The unchanged 180 MB settled typical-document budget passes after 5 MB prose/code use. Transient and large-document peaks remain reported separately; this is not an instantaneous RSS cap.

## Reading controls

Use the Find header button or Ctrl+F. Search is literal and case insensitive, includes prose, tables, code, formatted link labels and image descriptions, and shows the current match/count or zero matches. It excludes hidden link destinations. Next/previous wrap and reveal the active match, including inside embedded scrollers. Search highlighting preserves your copy selection. Queries debounce for 150 ms; snapshots and highlighting yield on the GTK thread, and Unicode case-folded matching runs on a cancellable GIO worker. All matches are counted and navigable; the first 1,000 and the active match receive highlights, with this limit explained in the count tooltip. Each window keeps its own query; Escape hides search and returns focus to the document.

| Shortcut | Action |
| --- | --- |
| Ctrl+O | Open a file |
| Ctrl+F | Show and focus Find |
| Ctrl+G / Ctrl+Shift+G | Next / previous match |
| Enter in Find | Next match |
| Ctrl++ (also Ctrl+=) / Ctrl+- | Increase / decrease text zoom |
| Ctrl+0 | Reset zoom to 100% |
| Ctrl+W | Close this window |
| Escape | Close Find (or cancel the file picker) |

The reading menu offers zoom and System/Light/Dark appearance. Text zoom ranges from 50% to 200% and applies to prose, links, tables and code. Theme and zoom update all open windows. System follows GTK's desktop appearance; manual overrides use GTK's named light/dark palettes while retaining native widget rules. Source documents and desktop settings are never changed.

Preferences live in `$XDG_CONFIG_HOME/mdview/preferences.ini`, or `~/.config/mdview/preferences.ini`. Theme and zoom save immediately; the last allocated non-fullscreen window size saves on close/quit. The compositor can override the restored size through tiling/placement. Missing, malformed, oversized or invalid preference values fall back to System, 100% and 850 × 650 (valid individual fields are retained). Failed saves are reported on stderr and leave the reader usable.

## Desktop integration and accessibility

The desktop entry, scalable icon and Markdown MIME declarations live under `data/`. The desktop file uses `nameh %F`; the executable must be on the launcher's PATH. GApplication handles activation, single-instance forwarding and multiple local file arguments. `DBusActivatable=false` deliberately uses executable activation; there is no D-Bus service file. The desktop entry's X11 class is `nameh`; Wayland uses the application ID.

Test a local installation without touching user directories or MIME defaults:

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo build --release --locked --offline
python scripts/check_desktop.py
python scripts/check_accessibility.py
```

The desktop check exercises the Makefile's actual install/uninstall in a temporary prefix, builds temporary desktop/MIME caches, exercises launcher and file-manager launch APIs on Wayland and X11, checks Unicode/multi-file forwarding, compares existing default associations and preserves an unrelated file during removal. It needs Hyprland, `make`, `gtk4-launch`, `gio`, `gdbus`, `xdg-mime`, `desktop-file-validate`, `update-desktop-database` and `update-mime-database`. The accessibility check needs PyGObject's Atspi binding, `at-spi2-registryd`, `dbus-daemon` and `wtype`; it creates its own accessibility bus and stops key input if Nameh loses focus. Close existing Nameh instances before either check. MIME defaults are always a user choice; no script calls `xdg-mime default`.

All reading controls support keyboard access. Native focus and caret styling are retained. Search counts and notices expose status roles with changing text; icon controls have accessible names. Tables expose aligned selectable text, rather than semantic row/cell navigation. Document text follows live system font changes as well as app zoom. GTK HighContrast/HighContrastInverse takes precedence over a saved Light/Dark choice; leaving high contrast restores that choice. Search's active text uses opaque selection colours and a double underline. Search and notice revealers have no animation. See the report for measured contrast, visual evidence and assistive-technology limits.

## Architecture

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Process entry point |
| `src/app.rs` | Application lifecycle, CLI opening, window registry and actions |
| `src/window.rs`, `src/window/reload.rs` | Worker opening, cooperative presentation, recoverable errors, GIO monitoring and reload coalescing |
| `src/window/reading.rs` | Search UI, reading keys, zoom styling and size tracking |
| `src/search.rs` | Rendered-text search and native match mapping/highlights |
| `src/reading.rs`, `src/appearance.css` | Shared reading actions and native palette overrides |
| `src/preferences.rs` | Validated, bounded preferences and atomic persistence |
| `src/picker.rs` | Lightweight native folder browser and virtualized file list |
| `src/state.rs` | State belonging to the window |
| `src/lib.rs` | Library entry point for display-independent parser tests |
| `src/document.rs` | GTK-independent owned document tree and pulldown-cmark conversion |
| `src/document/links.rs` | Safe link classification and local image target policy |
| `src/document_view.rs` | Native main text view, heading marks, embedded sizing, and clipboard expansion |
| `src/prototype.rs` | Bundled preview using the document model and native renderer |
| `src/native.rs` | Model-driven native document presentation |
| `src/images.rs` | Bounded, queued in-process raster decoding and percent decoding |
| `src/memory.rs` | Event-driven reclamation of freed glibc pages after native retirement |
| `src/render.rs` | Empty-state presentation |
| `src/files.rs` | Picker boundary and explicit desktop application launch |
| `src/navigation.rs` | Filesystem loading, once-only decoding and safe target resolution |

The application ID is `io.github.arash.mdview`. GTK supplies compatible GLib and GIO bindings through its re-exports. GTK APIs target 4.10 as a baseline rather than requiring the newest installed release. The application lockfile fixes resolved Rust dependencies. No browser runtime, editor widget, or background service is included.

## Document model

`nameh::document::parse(&str)` returns an owned `Document` containing the original source and a tree of `Node` values. Each node retains a half-open UTF-8 byte range into that source. Block and inline kinds preserve nesting, table alignment, code whitespace, link/image titles, image alt text, and ordered-list starts. Tight list items may contain inline nodes directly; loose items contain paragraphs. Table headers contain cells directly; body rows use `TableRow`. Only tables, task lists, and strikethrough extensions are enabled. Raw HTML events and HTML blocks are discarded; ordinary Markdown text between inline HTML tags is retained. Literal HTML inside code remains code text.

Heading IDs lowercase Unicode alphanumerics, retain underscores and hyphens, drop punctuation, and turn whitespace into hyphens. Empty names use `section`. Collisions get `-1`, `-2`, etc., checked against every previous ID, including headings with literal numeric suffixes. No Unicode normalization is applied. Heading text includes inline code, link labels, and image alt text.

`classify_link` distinguishes heading fragments, local paths, and explicit HTTP, HTTPS, or mailto URIs. Other schemes (including `file:`), scheme-relative URLs, backslashes, controls, and malformed percent escapes are rejected. Rejected links retain their labels with `target: None`; rejected or remote images retain their descriptions with `source: None`. Renderers must use these classified fields, never recover destinations from source slices. No parsing operation accesses files or dispatches URIs.

Paths and fragments remain percent encoded in the model. `src/navigation.rs` decodes each component once after splitting and resolves relative paths from the document directory. Every `LocalTarget` stays a filesystem path even when its decoded spelling resembles a URI. The same decoder is used for images; native picker and CLI paths are already filesystem paths and are not percent-decoded. No source slice is used to reconstruct rejected destinations.

File reads and parsing run on GIO workers. The window applies a successful result only if its request is still current. Files must be regular UTF-8 text; directories, unreadable paths, invalid encoding and remote GIO sources get explicit errors. Symlinks are followed by filesystem APIs. Before opening another file type externally, the resolved file is checked for readability, executable permission bits, known script/shortcut extensions and executable/launcher signatures. Non-Markdown file fragments are ignored; the native file launcher receives a filesystem-derived GIO file.

## Validation

Formatting, `cargo check`, strict Clippy, a release build and all 16 tests pass (including the extended single-thread GTK regression). The GTK regression covers rendering/copying, the actual picker action and selection/cancellation, folder navigation and filtering, GDK FileList drop signals, application Open/New Window actions, Unicode relative navigation and fragments, document preservation, Retry, launch errors and stale-result rejection. External dispatch tests use a recording launcher, so they do not open browsers or mail clients. Input bytes are compared after loading/navigation.

Real release-process checks passed for all three main fixtures through the CLI, forwarding to an existing instance, New Window, failed-open preservation and clean quit. Keyboard-driven picker selection passed for the same three fixtures, including the Unicode path, and cancellation preserved the document. A representative RSS sample after three picker selections and cancellation was 163.55 MiB (171.50 MB), within the 180 MB budget. See [rendering and opening notes](docs/RENDERING_PROTOTYPE.md) for the picker design, screenshot and measurement limits.

Step 6 adds preference default/validation/restart coverage and extends the GTK regression with Unicode/embedded search, formatted links, preserved selection, match wrap and scrolling, real key-controller dispatch, zoom bounds and multiple-window updates, appearance restoration and saved window defaults. Real release keyboard checks and theme/zoom restart passed. The final post-picker sample after exercising reading controls was **166.00 MiB / 174.07 MB** with GTK's default renderer, within the 180 MB budget. This is an idle sample; zoom/resize transients and large-document peak memory remain to be profiled. See the rendering report for measurement details and screenshots.

Step 7 adds real GIO ordinary/atomic/burst-save, deletion/rename/recreation, invalid UTF-8/Retry, symlink/parent-path, viewport, monitor cancellation and large-view/buffer release checks. Five-megabyte prose/search and code tests observed maximum main-loop gaps of 16.48 ms and 17.96 ms, below the 250 ms regression ceiling. Streaming Unicode matching preserves expansion offsets and full counts without a document-sized offset map. Image tests cover valid PNG/JPEG/GIF/BMP/PNM, alpha, scaling, security limits and skipping retired queued jobs.

Final release runs at 1520 × 956, with GTK's default renderer and complete checkpoint child accounting, returned from 5 MB prose/code to a small file after 20 seconds at **163.38 / 171.31** and **167.98 / 176.14** (MiB / MB), below the unchanged 180 MB budget. Normal and final picker checks also pass, with no children at any checkpoint. The [rendering report](docs/RENDERING_PROTOTYPE.md) records raw results, driver transients, the glibc reclamation decision, the raster decoder change, timing and remaining limits. Reproduce with `python scripts/measure_reload.py` and `python scripts/measure_reload.py --large-kind code`; `--output NEW_PATH` saves JSON lines. Allow about two minutes each, keep Nameh focused and use stable compositor geometry.

Run the full suite on a graphical desktop with session-bus access:

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo test --locked --offline -- --nocapture
```

The normal accessibility-bus refusal remains. Step 8 checks the current AT-SPI text, links, images, table/code reading order and actual keyboard reading controls on a temporary accessibility bus. Orca is not installed, so speech, announcements and full screen-reader usability remain unverified. Real pointer-drag selection/drop and actual browser/mail/other-file handlers remain manual checks; dispatch is regression tested. See the [Step 8 report](docs/RENDERING_PROTOTYPE.md#accessibility-and-desktop-integration-step-8).

Step 3 adds nine display-independent parser tests covering nesting, tables, heading collisions, source byte ranges, code, malformed Markdown, Unicode, HTML removal, and safe/unsafe link and image targets. Run these without a display:

```sh
export CARGO_HOME="$PWD/.cargo-home"
cargo test --locked --offline --test document --test navigation
```

The three navigation tests also run without a display and cover encoded paths/fragments, external-scheme rejection, readonly loading, unreadable/non-file/invalid-UTF-8 inputs, executable/shortcut rejection, symlinks, and unchanged source bytes.

## Fixtures

`fixtures/prototype.md` is the original Step 2 feasibility fixture, retained for copy/layout regressions. `fixtures/basic.md` covers basic syntax. `fixtures/rendering.md` combines tables, local and missing images, long code lines, and relative navigation for the complete renderer. `fixtures/notes/Unicode café.md` exercises Unicode and spaces in paths. `fixtures/assets/sample.ppm` is a tiny local raster image; no remote image is needed for the preview.

## Dependency references

- [gtk4 Rust API](https://docs.rs/gtk4/0.11.5/gtk4/)
- [pulldown-cmark Rust API](https://docs.rs/pulldown-cmark/0.13.6/pulldown_cmark/)

The raster decoder dependency is [image 0.25.10](https://docs.rs/image/0.25.10/image/), with default features disabled and only PNG/JPEG/GIF/BMP/PNM enabled. Existing GTK/pulldown-cmark lockfile versions remain unchanged.
