# Nameh Implementation Plan

Build a small Linux application for reading local Markdown files using Rust, GTK4, and pulldown-cmark. Implement one step per session, validate it, update this document, and finish with a copyable handover for a new session. Steps 1–9 are complete with recorded verification limits; see the latest session record and docs/RELEASE_VALIDATION.md. The user selected MIT and accepted the measured fresh-process startup results with OS caches warm/uncontrolled. No release publication or persistent installation has occurred.

## Project name and repository

The user chose **Nameh** as the app's name and initialized Git with commit
`5a3625d` (`first commit`). Git history is now available; inspect `git status` and
`git diff` before changing files and preserve unrelated work.

The implementation currently retains the working identifier `mdview` for the
Cargo package/executable, application ID `io.github.arash.mdview`, window/desktop
labels, installed asset names and preference paths. This documentation update
adopts Nameh as the product name; documented commands continue to match the code.
Earlier session records are historical: their statements about unavailable Git
history describe those sessions before the user initialized the repository.

## Session workflow

1. Read this document and any repository instructions. Inspect the existing code and working tree before changing anything.
2. Start the next unfinished step when the user asks to begin or continue. Complete only that step unless the user requests more.
3. Run the checks appropriate to the change. Record commands, results, and any manual checks that remain. Never describe an unperformed check as passing.
4. Update the progress table and append a session record below. Record architectural decisions beside the relevant step.
5. End with a brief outcome and a copyable handover using the template at the end. Stop before the following step.

If a step cannot be completed, leave it in progress and make the handover point to the exact remaining work. Do not mark it complete to fit a session boundary. Preserve unrelated user changes.

## Product requirements

- A viewer only. Never modify a document or show editing controls.
- One document per window, with an explicit action to open another window. Opening a file in an existing window replaces its document.
- Open through a file picker, drag and drop, or `mdview PATH`. Support paths containing spaces and Unicode.
- Render headings, paragraphs, emphasis, strikethrough, nested ordered and unordered lists, blockquotes, horizontal rules, inline and fenced code, tables, noninteractive task lists, links, images, and heading anchors.
- Provide selectable text, copy, document search, zoom, keyboard scrolling, and automatic reload after external changes.
- Resolve relative paths from the document directory. Open Markdown links internally and web links or other files through the default application.
- Follow system light or dark appearance with a manual override. Remember theme, zoom, and window size.
- Support keyboard access, visible focus, screen readers, high contrast, and reduced motion.
- Ignore raw HTML. Never execute scripts or executable URI schemes. Block remote images by default; enabling remote image loading is outside the initial scope.
- Make no background network requests and collect no telemetry. Browser navigation requires an explicit link activation.
- Exclude editing, tabs, workspaces, accounts, cloud sync, plugins, math, diagrams, embedded media, and syntax highlighting from the initial release.

### Performance targets

Measure a release build on a documented Linux reference machine. These are acceptance targets, not existing results.

| Measure | Target |
| --- | --- |
| Cold launch opening a 100 KB document | Readable within 1 second |
| Total resident memory for one typical document | Under 180 MB |
| Opening a 5 MB document | Interface remains responsive |
| Idle operation | No application polling timer or continuous background work |

## Technical approach

Use Rust and Cargo, GTK4 through gtk-rs, pulldown-cmark for parsing, and GIO for file monitoring and desktop integration. Store preferences in a small configuration file under `$XDG_CONFIG_HOME/mdview`, falling back to `~/.config/mdview`.

Parse Markdown into a small document model, then render it through native GTK text and widgets. Keep parsing, rendering, file handling, and application state separate. Use a read-only text view as the starting point; validate tables and embedded widgets before committing to its final layout. Avoid a browser runtime, a custom graphics engine, an extra application framework, and an async runtime unless a demonstrated requirement justifies them.

Choose compatible stable dependency versions during Step 1 and commit the application lockfile when version control is available. Keep raw HTML and unsafe links out of the rendering path rather than relying on visual hiding.

The user approved increasing the RSS budget from 100 MB to 180 MB during Step 2 after reviewing actual GTK measurements. The native GTK stack and default renderer are retained. Report MB and MiB explicitly when measuring.

## Progress

| Step | Deliverable | Status |
| --- | --- | --- |
| 1 | Runnable application skeleton | Complete |
| 2 | Native rendering feasibility prototype | Complete |
| 3 | Markdown parser and document model | Complete |
| 4 | Complete native document rendering | Complete |
| 5 | File opening and safe link navigation | Complete |
| 6 | Reading controls and preferences | Complete |
| 7 | Automatic reload and large document handling | Complete |
| 8 | Accessibility and Linux desktop integration | Complete |
| 9 | Release validation and packaging | Complete with documented verification limits |

## Step 1 Create the runnable application skeleton

Establish the smallest application that builds and opens a window.

- Inspect installed Rust, GTK4 development libraries, display access, and build tooling. Report any unavailable prerequisite precisely.
- Create a Cargo application named `mdview` and a GTK application with a stable application ID, compact header, open action, and empty state.
- Establish modules for application state, document parsing, rendering, and file handling without implementing later features.
- Add a README with prerequisites, build and run instructions, and the planned architecture.
- Add a small set of fixture documents covering basic formatting, a table, images, long code lines, Unicode, and relative links.

Completion checklist:

- [x] `cargo fmt --check`, `cargo check`, and `cargo clippy --all-targets -- -D warnings` pass.
- [x] Launch and action registration verified, with quit exit status 0. Visual inspection and picker interaction remain unverified; see the Step 1 session record.
- [x] Dependencies, application ID, and actual build commands are recorded.

Step 1 decisions: use application ID `io.github.arash.mdview`, GTK crate 0.11.5 with the GTK 4.10 API baseline, pulldown-cmark 0.13.6 without default features, and GTK re-exports for GIO and GLib. The document module is intentionally only a boundary until Step 3. The Open action selects a file and updates a status message without loading its contents.

## Step 2 Prove the native rendering approach

Resolve the difficult rendering questions before building the complete renderer.

- Render a representative fixture containing prose, an image, a table, a link, and a code block with long lines.
- Verify text selection and copying across paragraphs and around embedded content. Define a readable plain-text representation for copied table content.
- Verify horizontal scrolling for tables and code without widening the whole document.
- Check resizing, keyboard focus, and basic screen reader exposure for the prototype.
- Compare observed startup and memory use with the targets to catch an unsuitable approach early.

Completion checklist:

- [x] The prototype demonstrates the hard cases with native GTK rendering.
- [x] The chosen widget structure and its selection, copying, and accessibility behavior are documented in `docs/RENDERING_PROTOTYPE.md`.
- [x] Any unresolved requirement blocks completion of this step. The user explicitly approved raising the RSS budget to 180 MB; no stack change was made. Remaining manual verification limits are recorded in the prototype report.

Step 2 decisions: one main read-only TextView with embedded scrollers for aligned table text and unwrapped code, embedded pictures and link buttons, and an anchor-aware Copy handler. Main-document selections copy entire crossed blocks; selections within a block remain local. Keep this as a feasibility reference while implementing the independent document model in Step 3 and the complete renderer in Step 4.

## Step 3 Implement parsing and the document model

- Convert pulldown-cmark events into a document model independent of GTK.
- Enable tables, task lists, and strikethrough explicitly. Represent all required block and inline elements.
- Define deterministic heading IDs, including duplicate headings, and retain source positions useful for reload anchoring.
- Define link classification and allowed URI schemes. Support local paths, heading fragments, `http`, `https`, and `mailto`; reject executable and unknown schemes.
- Ignore raw HTML events and retain ordinary Markdown text around them.
- Add focused tests for nesting, tables, duplicate headings, malformed input, Unicode, and unsafe links.

Completion checklist:

- [x] Parser tests cover required syntax and security boundaries.
- [x] The model contains no GTK dependencies.
- [x] Raw HTML and unsafe link targets cannot become active rendered content.

Step 3 decisions: expose `mdview::document` through `src/lib.rs` for tests without GTK initialization. Use an owned node tree with distinct block/inline kinds and half-open source byte ranges, retaining the original source. Tight lists may contain inline nodes directly; table headers contain cells directly. Enable only tables, task lists, and strikethrough. Ignore HTML events and whole raw HTML blocks; preserve literal HTML in code. Heading IDs lowercase Unicode alphanumerics, retain `_` and `-`, remove punctuation, collapse whitespace into hyphens, and use `section` when empty; global collisions append numeric suffixes, including collisions with literal suffixes. No Unicode normalization is applied.

Links carry classified targets (local, heading, HTTP/HTTPS/mailto), or `None` for inert labels. Images carry local targets only, or `None` for placeholders. Reject other schemes, scheme-relative URLs, backslashes, controls, and invalid percent escapes. Paths and fragments stay encoded; Step 5 must decode once after splitting, resolve against the document directory, and use filesystem APIs for local paths even if their decoded spelling resembles a URI. Renderers must never extract a rejected destination from a source slice. File existence, Markdown-vs-other-file navigation, indirect remote image references, decoding limits, and actual dispatch remain for Steps 4–5. The prototype remains temporary direct rendering until Step 4.

## Step 4 Complete native document rendering

- Replace prototype code with the renderer selected in Step 2.
- Render every supported Markdown element with readable spacing and a centered content column.
- Preserve code whitespace, show noninteractive task boxes, and load local images with sensible size limits and error placeholders.
- Keep remote images blocked, including indirect remote references. Prevent image decoding from stalling the UI and handle malformed or oversized images gracefully.
- Implement heading destinations, link hit testing, and selection and copying behavior.

Completion checklist:

- [x] All rendering fixtures are readable at narrow and wide window sizes.
- [x] Selection and copying meet the behavior established in Step 2.
- [x] Missing images, malformed Markdown, and long lines do not break the layout.
- [x] Rendering causes no document writes or network requests. The renderer uses filesystem reads and raster-only decoders; default URI activation is intercepted. Broader runtime tracing remains in Step 9.

Step 4 decisions: `src/native.rs` consumes only the owned model and classified targets. Keep the Step 2 DocumentView/scroller/copy structure, with centered 900-pixel content, styled text and table cells, heading TextMarks, noninteractive task glyphs, and native LinkButtons for hit testing/focus. Heading activation decodes fragments once and targets the root document even from an embedded table. All link-button default URI launching is stopped; the caller-supplied callback receives other classified targets for Step 5. Rejected destinations remain inert and are never reconstructed from source ranges.

Local raster reads and decoding use GIO blocking workers, a serialization lock, regular-file checks, bounded reads (8 MiB), pre-decode header checks (16 million pixels, dimensions ≤16,384), and size-prepared checks. Retained pixel data is capped to 1024 × 768 independently of decoder scaling. Allow PNG/JPEG/static GIF/BMP/PNM signatures only; reject animated GIF and vector/container formats such as SVG to prevent frame accumulation and indirect remote fetching. Use filesystem paths after exactly one decode; never reinterpret a LocalTarget as a URI. Decode failures and blocked sources remain accessible placeholders. The preview now renders all three main fixtures through the production renderer and reads its images from the repository fixture directory; arbitrary file opening remains Step 5.

## Step 5 Implement file opening and navigation

- Connect the picker, drag and drop, and `mdview PATH` to a shared file-opening path.
- Support Unicode and spaces in filenames and report invalid paths, unreadable files, directories, and invalid text encoding clearly.
- Preserve the current document when opening another file fails; show the error with a retry action.
- Resolve relative image and link targets against the document directory. Handle percent-encoded paths and fragments consistently.
- Follow internal anchors and relative Markdown links inside the viewer. Dispatch supported external links and other file types through system applications only after activation.
- Implement one-document-per-window behavior and an explicit New Window action.

Completion checklist:

- [x] All three opening methods work with the same fixtures. Picker and CLI were exercised in release processes; drops use the installed GDK FileList signal handler in the GTK regression. Real pointer dragging remains a Step 8 manual check.
- [x] Relative navigation, fragments, and unsafe scheme rejection have focused tests.
- [x] File opening and link activation never alter the source file. Fixture/source bytes are compared after navigation; the application has no document-write path.

Step 5 decisions: `src/navigation.rs` is display independent and owns filesystem loading, once-only decoding, Markdown extension detection, target resolution and checks for other-file dispatch. `src/window.rs` owns each window's shared request pipeline. Picker, dropped GIO files and GApplication Open events converge on `open_file`; parsing/reads run on GIO workers and request generations reject stale results. Replacement happens only after successful loading. Errors have a dismissible message and Retry; a missing heading in a readable destination reports the fragment error after opening that document. Native CLI/picker paths are not percent-decoded. Classified local link paths/fragments are decoded once; URI-like decoded names remain filesystem paths. Non-Markdown fragments are ignored. External files use the canonical filesystem path after regular-file/readability/executable/script/shortcut checks, including symlink targets. HTTP/HTTPS/mailto launch only on activation, through GTK/GIO launchers with error recovery.

Use a compact local folder browser (`src/picker.rs`) with native GTK Entry/ListView/SingleSelection/buttons, a virtualized file model, worker directory/path checks, Up, All files, Ctrl+L, Enter, and Escape/Cancel. The default desktop portal failed keyboard path confirmation with backend tree-model errors. The stock in-process chooser worked but retained roughly 186–196 MB RSS after repeated use, above the approved budget. The lightweight picker keeps the final sample at 171.50 MB without renderer overrides, deprecated chooser APIs, allocator FFI, or a helper process. GTK default renderer and 180 MB target remain unchanged. The application registry releases controllers on close; CLI opening reuses the active normal window and creates separate windows for additional files. Preview windows retain their fixture-only opening policy.

## Step 6 Add reading controls and preferences

- Implement find over rendered document text, next and previous matches, match count, and scrolling to the active match.
- Implement zoom in, zoom out, and reset with reasonable bounds.
- Implement system appearance, light and dark overrides, and preference persistence.
- Save window size and handle missing or malformed preferences using defaults.
- Add shortcuts: `Ctrl+O`, `Ctrl+F`, `Ctrl+G`, `Ctrl+Shift+G`, `Ctrl++`, `Ctrl+-`, `Ctrl+0`, `Ctrl+W`, and `Esc`.
- Preserve normal scrolling and copy shortcuts. Keep menus compact.

Completion checklist:

- [x] Search covers text inside tables and code, handles Unicode, and reports zero matches correctly.
- [x] All shortcuts work without conflicting with selection or search input.
- [x] Theme, zoom, and window size survive a restart.
- [x] Invalid preferences do not prevent startup.

Step 6 decisions: `src/search.rs` builds a temporary rendered-text index in document order and maps Unicode character ranges back to native buffers and anchor widgets. Search is literal, case insensitive and nonoverlapping, wraps in either direction, and includes prose, formatted link labels, table cells, code and image descriptions. Link destinations are copy metadata and are excluded. Text tags and widget backgrounds highlight matches without changing copy selection. Root and embedded scrollers reveal the active match. Search is per window; theme and text zoom are shared across windows. Search indexing remains synchronous; large-document work stays in Step 7.

Use a plain native GTK Entry in a Revealer without a transition for search. Full SearchBar transitions and complete replacement widget themes caused extra renderer allocations on the reference NVIDIA desktop. Appearance imports only GTK's built-in Adwaita named palette plus app-scoped role rules, retaining the desktop widget rules. System mode removes the provider and follows GTK's live desktop appearance. No renderer or desktop settings are changed. `Ctrl+G`/`Ctrl+Shift+G` also use a window key controller accepting both G key values so they work from the search entry; ordinary copy and scrolling pass through.

Text zoom is 50–200% in 10-point increments, with 100% reset. Window-scoped font CSS covers prose, links, tables and code; embedded heights are remeasured after CSS updates. Preferences use GLib KeyFile at `$XDG_CONFIG_HOME/mdview/preferences.ini` (fallback `~/.config/mdview/preferences.ini`), bounded reads, individual field validation and atomic private GIO replacement. Defaults are system appearance, 100%, 850 × 650. Save theme/zoom immediately and the last allocated non-fullscreen size on close/shutdown. Size changes are observed through actual surface layout events, with unchanged dimensions ignored. Tiled windows on this desktop report maximized; their allocated size is retained. GTK requests the restored dimensions, but compositor placement/tiling can override the resulting allocation. Save errors go to stderr and do not prevent reading.

## Step 7 Add automatic reload and responsive loading

- Monitor the document through GIO notifications, including atomic replacement by external editors.
- Coalesce bursts of changes without permanent polling. Handle deletion, rename, and temporary unreadability with a retry path.
- Preserve reading position through a nearby source or heading anchor, falling back to a proportional scroll position.
- Keep file reading and parsing off the UI thread where necessary; apply GTK updates on its main thread in bounded work units.
- Prevent stale background results from replacing a newer document. Release old monitors and document resources.

Completion checklist:

- [x] Ordinary saves and atomic saves reload correctly.
- [x] Repeated reloads preserve a useful reading position, cancel old monitors and release retired views; typical-document samples settle after 30 reloads, and large prose/code views and buffers are released.
- [x] At-least-5,000,000-byte prose and code fixtures load while the window responds to input; GTK heartbeat and reading-control dispatch regressions pass.
- [x] Idle operation requires no application polling timer.
- [x] Resolve the unchanged 180 MB settled typical-document RSS failure after large-document use and profile reload transients. See the latest session/report for raw samples and explicit peak/cache limits.

Step 7 decisions: monitor canonical parent directory entries with GIO `WATCH_MOVES`, matching either event endpoint; retain the logical document path and additionally watch resolved symlink targets. Use a replaceable 150 ms quiet-period timeout, never polling. Keep the last readable view on errors and rearm from later path events or Retry. Cancel superseded presentation futures immediately and retain request-generation rejection for worker responses. Defer dirty old-document events during explicit navigation, and recheck inode/device/size/mtime on a worker after monitor installation to catch saves during initial loading.

Render cooperatively into an unattached native view, replacing the visible view only after completion. Text/code insertion uses UTF-8-safe 8 KiB chunks; node/list/table loops yield around 4 ms, code insertion yields per chunk. GTK calls remain on the main thread; parsing/reads remain GIO workers. Disable GTK's default undo history for all read-only document buffers; give parser containers a one-node initial allocation and retire completed models on a worker. The memory-acceptance session revises the original no-FFI/no-new-dependency decisions: large-view disposal and picker close on GNU/Linux request glibc `malloc_trim(0)` on a worker after a coalesced temporary 100 ms callback. Other libc targets use a no-op and are not covered by this memory acceptance. Application images use `image` 0.25.10 with only the existing five raster formats, replacing the retained platform decoder process. An image queue holds weak widget references, skips retired jobs and permits one active decoder. Existing file/header/pixel/GIF limits remain; the reader also configures a decoder-dependent, non-strict 256,000,000-byte allocation limit and retains ≤1024 × 768 pixels. This supersedes Step 4’s platform decoder/size-prepared implementation. The existing GTK default renderer, scroller/CSS/picker geometry and all behavior remain unchanged. No application helper process or allocator replacement/tuning is added.

Search debounces 150 ms, snapshots native buffers in bounded chunks, streams Unicode case-folded matching on cancellable workers with query-sized prefix/history arrays, stores compact character ranges, and limits background highlight tags to the first 1,000 hits plus the active hit. Counts/navigation include every match; the tooltip explains the highlight limit. Scroll through retained marks to reveal matches in previously unvalidated large buffers. Reload recomputes a query without revealing the first hit over the restored viewport. Restore a nearest-heading plus relative-line position after allocation; use proportional scrolling when its heading disappears. Ordinary source writes, path decoding, picker behavior, embedded scrollers/copy, zoom/theme/preferences and safe dispatch policy are unchanged.

## Step 8 Finish accessibility and Linux integration

- Verify labels, roles, focus order, keyboard activation, and screen reader reading of document text, links, images, and tables.
- Check high contrast, large fonts, light and dark modes, and reduced motion.
- Add a desktop entry, application icon, Markdown MIME declarations, and launch activation handling.
- Confirm opening from a file manager and behavior under available Wayland and X11 environments. Record unavailable environments.
- Keep MIME registration user controlled; installation must not silently replace existing default applications.

Completion checklist:

- [x] Core reading tasks work using only the keyboard.
- [x] Screen reader and contrast checks are recorded with actual results: current AT-SPI reading passes; Orca/speech review is unavailable, explicitly recorded.
- [x] Desktop launch and file-manager launch APIs work after an isolated local installation on Wayland and X11; actual file-manager GUI gestures remain manual.

Step 8 decisions: retain GTK 4.10 baseline, native text/link/image roles and theme focus styling. Expose dynamic counts/notices as Status and label icon controls explicitly. Recompute document font size from inherited GTK font settings on live changes; high-contrast GTK themes (including explicit `GTK_THEME`) take precedence over manual palettes. Opaque active selection colours plus a double underline avoid relying on colour alone. Both revealers are unanimated. Tables remain accessible aligned text, without semantic cell navigation. A freedesktop entry uses `mdview %F`, executable activation (`DBusActivatable=false`) and the existing GApplication activation/Open pipeline; no D-Bus service is installed. The SVG icon follows the app ID, and X11 StartupWMClass is `mdview`. MIME XML adds Markdown extensions and an alias without choosing defaults. Development checks install only under `/tmp` and remove their caches/assets afterward; release packaging stays Step 9.

## Step 9 Validate and package the first release

- Run formatting, lint, build, and meaningful parser, navigation, and state tests.
- Exercise an end-to-end checklist covering opening, rendering, search, copying, links, zoom, preferences, reload, and error recovery.
- Measure release-build startup and resident memory. Record hardware, distribution, display session, fixture, measurement method, and results. Include child processes in memory accounting if present.
- Verify source files remain unchanged and that opening documents makes no unsolicited network requests.
- Resolve any acceptance failures before marking the release complete. Record unavailable verification separately from failures.
- Provide a reproducible native build and installation path, uninstall instructions, a license selected by the user before distribution, and a short user guide. Additional distribution formats can follow after the initial release.

Completion checklist:

- [x] All available functional checks pass the release checklist; unavailable speech, physical gestures and actual external handlers are explicitly unperformed, not claimed as passing.
- [x] Performance targets have measured results; the user explicitly accepted fresh-process startup measurements with warm/uncontrolled OS caches. All fixed typical-document idle checks pass; large-use/transient peaks are separate.
- [x] A clean build and local install can be reproduced from the README.
- [x] Known limitations and any unperformed checks are clearly recorded.

Step 9 decisions: native Makefile build/install/uninstall with PREFIX and
DESTDIR staging, selected MIT license and installed user guide. Cache updates use
available desktop tools and never select a MIME default; staging suppresses cache
updates. No archive, registry publication or persistent installation occurs.
Startup measurement captures rendered content, rather than treating a mapped
window as proof of readability. The user explicitly accepted the measured fresh
processes with no running instance and warm/uncontrolled OS caches for the startup
criterion. Cold OS-cache performance remains unperformed and is not claimed. The
Linux x86-64 ptrace check follows all process/thread descendants and validates its
Internet/Unix socket controls before opening documents. Its scope and manual
verification limits are recorded in docs/RELEASE_VALIDATION.md.

## Session records

### Planning session

- Completed: agreed product scope, proposed architecture, implementation sequence, completion criteria, and handover format.
- Changed: `IMPLEMENTATION_PLAN.md`.
- Validation: reviewed the plan against the earlier specification and stack proposal. No application code exists yet.
- Next: Step 1.

### Step 1 session

- Completed: runnable GTK application with compact header, empty state, accessible Open button label, file picker, filename acknowledgement, Ctrl+O, and Ctrl+Q. Established separate application, state, document, rendering, and file modules.
- Changed: `Cargo.toml`, `Cargo.lock`, `.gitignore`, six files under `src/`, `README.md`, four files under `fixtures/`, and this plan.
- Environment: Rust and Cargo 1.99.1, GTK 4.24.1, Wayland session with X11 display also advertised. Arch packages `rust`, `gtk4`, `pkgconf`, and `base-devel` were already installed. No additional system packages were required.
- Dependencies: lockfile resolves gtk4 0.11.5, pulldown-cmark 0.13.6, GIO 0.22.10, and GLib 0.22.10. There is no usable Git repository; the existing `.git` directory is read-only, so the lockfile is retained on disk without a commit.
- Validation passed: `cargo fmt --check`; with `CARGO_HOME="$PWD/.cargo-home"`, `cargo check --locked --offline`, `cargo clippy --locked --offline --all-targets -- -D warnings`, and `cargo build --locked --offline`.
- Launch check passed: a temporary Python harness launched `target/debug/mdview`, queried the window's exported Open action over D-Bus, activated the application quit action, and observed exit status 0. It ran on both an isolated session bus and the normal desktop bus. No smoke-test hook was added to the application.
- Limitations: visual layout, picker selection and cancellation, and keyboard interaction were not manually verified. GTK reported accessibility bus failures, including connection refused at `/run/user/1000/at-spi/bus_0` on the normal bus; screen reader verification remains pending. This warning did not prevent action registration or process exit. X11 launch was not tested.
- Build access: initial crates.io DNS failed inside the sandbox. An approved network-enabled Cargo invocation downloaded dependencies. Subsequent checks succeeded offline using the project-local Cargo home. Graphical smoke checks needed access outside the sandbox to the desktop bus and display.
- Next: Step 2, the native rendering feasibility prototype. Test tables, copying across content, images, long code scrolling, resizing, keyboard access, and accessibility before selecting the final widget structure. Measure a release build for the preliminary performance check.

### Step 2 session

- Completed: `--prototype` mode rendering a bundled representative fixture with styled prose, an aligned table, a raster image, long code lines, Unicode, and an HTTPS link. Implemented a TextView subclass that constrains embedded widths and expands embedded contents during Copy. Fixed short-block clipping and enforced a monospace tag for stable table columns under desktop styling. The prototype Open action is disabled; normal skeleton behavior remains available without the flag.
- Changed: `src/main.rs`, `src/app.rs`, new `src/document_view.rs` and `src/prototype.rs`, `fixtures/prototype.md`, `scripts/measure_prototype.py`, `docs/RENDERING_PROTOTYPE.md`, `docs/prototype.png`, `README.md`, and this plan. No new dependencies or system packages were required.
- Validation passed: `cargo fmt --check`; with the local Cargo home, `cargo check --locked --offline`, `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo build --release --locked --offline`, and `cargo test --locked --offline -- --nocapture` on the graphical desktop. Clipboard round trips, Unicode, code indentation, selection around whole embedded blocks, individual block copying, independent horizontal adjustment, no short-block vertical clipping, widths 400/900/360, read-only state, and Tab traversal through both blocks and the link passed.
- Accessibility: actual AT-SPI inspection on a dedicated temporary accessibility bus exposed document text, table rows, code contents, image description, and link role and label. The reference desktop's normal accessibility bus remains unavailable; full screen reader usability review belongs to Step 8. No desktop configuration was changed.
- Visual review: inspected an app-only screenshot after correcting table clipping and monospace alignment. The screenshot is retained in `docs/prototype.png`. Real key events through `wtype` verified Ctrl+Home, Ctrl+Shift+End, Ctrl+C, and Ctrl+Q; UTF-8 clipboard data contained both embedded blocks, the image description, link, and Unicode intact. The generic plain-text clipboard target escaped non-ASCII bytes on this installation, while explicit UTF-8 and GTK text reads worked. Pointer-drag selection remains unverified; the integration test covers the corresponding buffer ranges. GTK focus traversal also passed.
- Performance: final optimized prototype, five fresh processes with warm caches and the 1,372-byte fixture, median mapped-window latency 310.5 ms and median RSS 158.16 MiB; maximum observed RSS 158.64 MiB, approximately 166.35 MB. No child processes remained when sampling RSS. Cairo comparison reduced median RSS to 107.23 MiB. These are preliminary figures, not proof of the cold 100 KB launch target.
- Scope decision: after reviewing the memory results, the user chose to retain GTK and raise the RSS budget to 180 MB. Kept the default renderer and avoided programmatic GTK debug environment overrides. No unresolved architecture blocker remains for the next step.
- Next: Step 3, the GTK-independent Markdown document model and parsing tests. Keep prototype parsing temporary; do not promote its limited syntax and fixture-only image policy into the complete parser. Step 4 will replace direct parsing with model-driven rendering.

### Step 3 session

- Completed: GTK-independent owned document model, pulldown-cmark event conversion for all required syntax, source byte ranges, deterministic unique Unicode heading IDs, conservative link classification, local-only image targets, and nine focused parser integration tests. No file reads, image decoding, network requests, or navigation were introduced. Stopped before Step 4.
- Changed: `src/document.rs`, new `src/document/links.rs`, new `src/lib.rs`, `src/main.rs` (removed the placeholder binary module declaration), new `tests/document.rs`, `README.md`, and this plan. No dependency, lockfile, renderer, desktop, or package changes. Repository instruction search found no applicable AGENTS.md; the `.git` directory remains an empty read-only mount with no usable Git history.
- Validation passed: `cargo fmt --check`; with `CARGO_HOME="$PWD/.cargo-home"`, `cargo check --locked --offline`, `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo test --locked --offline --test document` (9 tests), `cargo build --release --locked --offline`, and full `cargo test --locked --offline -- --nocapture` on the graphical desktop (9 parser tests plus the existing GTK integration test).
- Test coverage: nested quotes/lists/emphasis and task markers, table cells/alignment/formatting, all heading levels and setext headings, duplicate and literal-suffix heading collisions, Unicode, byte-range slicing and containment, inline/fenced/indented code and whitespace, breaks/rules, malformed/unclosed input, existing fixtures, reference links/autolinks/titles, HTML removal, entity-obfuscated executable schemes, remote images, unknown schemes, controls, backslashes, and invalid escapes.
- Environment limits: the first full test attempt inside the sandbox could not initialize GTK. The same command passed with approved graphical desktop access. GTK still reported the known normal accessibility-bus connection refusal. No new screen reader, pointer-drag, picker, screenshot, or performance checks were performed; Step 2 results and their limits remain the reference.
- Next: Step 4 only. Replace temporary direct prototype parsing with model-driven native rendering; preserve embedded scrollers and anchor-aware copying, implement the remaining syntax and heading destinations, and add safe local image loading with size/error handling. Retain GTK's default renderer and the approved 180 MB RSS budget. Step 5 will connect general file opening/navigation.

### Step 4 session

- Completed: model-driven native rendering for all supported syntax, centered content, styled/aligned table cells, preserved code whitespace, nested lists and ordered starts, noninteractive task markers, heading destinations and encoded Unicode fragments, native link hit testing/focus, safe local raster loading, error placeholders, and the existing embedded selection/copy behavior. Stopped before Step 5; normal picker selection still only acknowledges a file.
- Changed: new `src/native.rs` and `src/images.rs`; replaced direct parsing in `src/prototype.rs`; extended `src/document_view.rs`; registered modules in `src/main.rs`; removed stale rendering-status wording in `src/app.rs` and `src/render.rs`; updated `README.md`, `docs/RENDERING_PROTOTYPE.md`, this plan, and added `docs/native-rendering-narrow.png` and `docs/native-rendering-wide.png`. No dependencies or lockfile changes. No applicable AGENTS.md and no usable Git history; empty read-only `.git` mount persists.
- Validation passed: `cargo fmt --check`, `cargo check --locked --offline`, strict `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo build --release --locked --offline`, nine parser tests, and full `cargo test --locked --offline -- --nocapture` with graphical desktop/session-bus access (9 parser tests plus the extended GTK regression and one image-safety test). The first sandboxed full run failed because GTK and the platform raster loader could not access the display/session bus; the desktop run passed. The documented `cargo run --locked --offline -- --prototype` command also launched successfully and quit through its application action with status 0.
- GTK coverage: existing clipboard round trips, literal U+FFFC, block-local copying, code whitespace, independent scrollers, read-only state, focus traversal, and narrow/wide resizing; new full-fixture rendering at 360/400/1000 pixels, ordered starts and nested lines, heading existence/collisions, once-decoded Unicode fragment activation in prose and tables, stopped default URI launching, inert rejected targets, asynchronous local images, and missing/blocked placeholders. Fixed trailing-newline code-height accounting and native wrapped link sizing after regression/visual checks. Snapshot capture was temporary test instrumentation, removed from source after producing the saved PNGs.
- Image coverage: fixture raster decode, regular-file/byte/pixel/dimension limits, missing and malformed images, indirect-remote SVG rejection, single-frame GIF validation, unchanged input bytes, and encoded filesystem paths resembling URIs. Code inspection confirms no document write or network/URI-dispatch API in the rendering path; comprehensive runtime network tracing remains for Step 9.
- Visual checks: inspected GTK-rendered snapshots at 360 and 1000 logical pixels covering basic formatting, lists/tasks/quotes/rules, formatted tables, local/missing images, long code, links, Unicode and headings. Saved representative narrow/wide snapshots in docs. Full screen reader review, actual pointer-drag selection, and picker interaction remain unverified. Known accessibility-bus refusal persists; one test run also reported a GDK discarded-frame timing warning without failure.
- Performance: release build, GTK default renderer, same reference machine/method as Step 2, combined bundled preview (2,742 bytes including added separators). Five final warm-cache runs mapped in 304.1–351.1 ms (median 323.8 ms), with RSS 157.82–158.25 MiB (median 158.12 MiB; maximum about 165.94 MB), within the approved 180 MB budget. No remaining child processes at sampling. This does not establish cold launch, 100 KB readable latency, 5 MB responsiveness, or arbitrary-image peak memory.
- Next: Step 5 only. Connect picker, drag/drop and CLI paths to shared file opening, preserve a document on errors, implement New Window and classified safe navigation. Call `native::render` with the document directory and a dispatch callback; reuse the once-only decode/filesystem policy in `images.rs`. Keep heading activation, scrollers, anchor-aware copying and default GTK renderer. Stop before Step 6.

### Step 5 session

- Completed: shared picker/drop/CLI opening, Unicode/space paths, readable filesystem/UTF-8 errors, preservation on failure with Retry, relative Markdown navigation and decoded heading fragments, same-document/top anchors and missing-heading feedback, explicit HTTP/HTTPS/mailto and other-file desktop dispatch, New Window, active-window replacement and one-document-per-window multi-file opening. Added Ctrl+N and Ctrl+W alongside existing Open/Quit shortcuts. Stopped before Step 6.
- Changed: new `src/navigation.rs`, `src/window.rs`, `src/window/tests.rs`, `src/picker.rs`, and `tests/navigation.rs`; updated `src/app.rs`, `src/state.rs`, `src/files.rs`, `src/lib.rs`, `src/main.rs`, `src/images.rs` (shared decoder), `src/native.rs` (missing-heading callback), and `src/document_view.rs` (top anchor/test integration). Updated README, this plan, rendering/opening report and added `docs/file-picker.png`. Dependencies and lockfile are unchanged. No applicable repository instructions or usable Git history were found; the empty read-only Git mount persists.
- Validation passed: `cargo fmt --check`, `cargo check --locked --offline`, strict `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo build --release --locked --offline`, and full `cargo test --locked --offline -- --nocapture` with desktop access (9 parser tests, 3 navigation tests, image-safety test, and extended GTK regression: 14 total). Display-independent parser/navigation tests also passed offline without desktop access.
- GTK coverage: actual Open action/picker callback for basic/rendering/Unicode fixtures; folder listing/Up/row activation/filter/All files and path errors; Cancel preserving the document; installed GDK drop handler for the same fixtures and multi-drop rejection; application Open, active-window reuse, New Window, multi-file windows and controller release; relative encoded Unicode links and cross-file heading selection; invalid files and remote sources preserving the old view; Retry after creating a missing file; missing headings/top fragment; external handler success/failure/retry through a recording launcher; inert unsafe schemes, script rejection, unchanged source bytes and newest-request wins. Tests stay on one GTK thread and isolate recent-file state.
- Real desktop checks: release CLI opened all three main fixtures, forwarded paths to an existing process, replaced the active window, retained documents/titles on an unreadable target, created a new window and quit with status 0. Real keyboard input selected basic/rendering/Unicode fixtures through the final native picker and Escape preserved the current document. Inspected and saved the picker snapshot. External browser/mail/other-app handlers were not actually launched during tests; their dispatch and errors use a recording launcher. Real pointer dragging for selection and drops, full screen reader review and broad desktop handler integration remain for Step 8.
- Picker/performance decision: portal confirmation failed on this desktop; the stock in-process chooser exceeded 180 MB after repeated use. Replaced it with a lightweight virtualized GTK file list and worker filesystem operations. A final release sample one second after three real picker selections and cancellation measured 163.55 MiB / 171.50 MB on the Step 2 reference desktop with GTK's default renderer. This is a representative idle sample, not a peak-memory/cold-launch/100 KB/5 MB acceptance result. There is no remaining chooser helper or allocator workaround. A temporary measurement example and snapshot instrumentation were removed from source.
- Environment notes: final full suite reports only the known desktop accessibility-bus refusal. Intermediate chooser experiments exposed portal/model warnings and a GTK recent-file test-harness crash; the final picker and test initialization avoid that path. A real-keyboard run also emitted the previously seen discarded-frame timing warning without failure. No desktop service/settings changes were made. Comprehensive runtime network tracing remains for Step 9; rendering, loading and resolution perform no URI dispatch or network fetches, and only explicit link activation reaches desktop launchers.
- Next: Step 6 only. Add search across prose and embedded blocks, zoom, system/light/dark appearance and persisted theme/zoom/window size, plus reading shortcuts. Preserve the shared request pipeline, readonly source behavior, safe target policy, lightweight picker, native renderer, embedded scrollers, heading destinations and anchor-aware copying. Stop before Step 7; automatic reload and fully bounded large-document rendering remain there.

### Step 6 session

- Completed: per-window literal Unicode case-insensitive search over rendered prose, formatted link labels, tables, code and image descriptions; all/active match highlights, count/zero state, next/previous wrap and scrolling into embedded blocks. Added text zoom (50–200%, 10-point increments, reset), System/Light/Dark appearance, shared live settings across windows, persisted preferences/window size, compact reading menu and all requested shortcuts. Stopped before Step 7.
- Changed: new `src/search.rs`, `src/reading.rs`, `src/preferences.rs`, `src/appearance.css`, and `src/window/reading.rs`; updated `src/app.rs`, `src/window.rs`, `src/window/tests.rs`, `src/document_view.rs`, `src/main.rs` and `src/lib.rs`. Updated this plan, README and rendering report, with reading-control screenshots. Dependencies/lockfile, picker, navigation, worker loading and native rendering policy are unchanged. No usable Git history or additional repository instructions were available.
- Validation passed: formatting, offline locked check, strict Clippy, release build, display-independent parser/navigation tests, and full desktop suite (15 tests: 9 parser, 3 navigation, preference validation/persistence, image safety and extended single-thread GTK regression). The GTK test checks Unicode/zero/empty queries, formatted link boundaries, destination exclusion, match wrap, code scroll positioning, copy-selection preservation, link highlights, uppercase/lowercase Ctrl+Shift+G and Escape dispatch, native copy passthrough, zoom bounds/font changes and all-window updates, Light/Dark palette change, System restoration, invalid theme rejection, and a restarted controller requesting saved dimensions/zoom. All earlier opening/picker/navigation/copy tests continue to pass.
- Real desktop validation: optimized binary, default GTK renderer, isolated XDG config with the existing desktop GTK config linked for consistent styling. Real Ctrl+F/G/Shift+G, Ctrl++/=/minus/0, Escape, Ctrl+W/Q and Ctrl+O picker flows passed. Inspected screenshots showing 1→2→1 search counts and embedded highlights, light/dark headers/content and 140% zoom after restart. Saved theme=dark, zoom=140, width=960 and height=720 and verified restart; GTK's requested dimensions are regression tested while Hyprland overrides actual tiling/floating allocation. Opened basic/rendering/Unicode fixtures with the preserved picker and cancelled the following picker. No real external desktop handler was launched.
- Performance: on the documented Step 2 reference machine, final release-process sample one second after those picker operations/cancellation was **166.00 MiB / 174.07 MB**, final document `fixtures/notes/Unicode café.md`, 100% zoom, System appearance; no child processes remained. In the same final run, settled 140% zoom on rendering.md sampled 176.66 MB. These are representative samples, not peak/cold-launch/100 KB/5 MB acceptance results. An earlier short-delay 140% resize sample was 180.24 MB; zoom/resize transients and arbitrary large content still need Step 9 peak profiling. Intermediate full-theme providers/search transitions exceeded the budget (about 195 MB) and produced pixman damage warnings; those implementations were replaced by the plain entry/no-transition row and palette-only override. The final full suite has no pixman warnings.
- Environment/limits: initial sandbox-only full tests could not access GTK/session D-Bus; desktop-enabled runs pass. The known accessibility-bus refusal remains; an occasional discarded-frame warning appeared in real runs and the final full suite. Full screen reader/high-contrast review, pointer dragging/drop and actual default-handler integration remain Step 8. System restoration to the current live GTK palette was tested; no desktop-wide theme switch was performed. Search currently builds a temporary GTK text buffer and range map on the UI thread; reload monitoring, burst coalescing, large-document UI bounds and reading-position restoration remain Step 7. No permanent timer, allocator workaround, renderer override or document-write path was introduced.
- Next: Step 7 only. Implement GIO monitoring including atomic replacement, coalescing and deletion/temporary-error recovery; preserve reading position, avoid stale reload results, bound main-thread presentation/search work for 5 MB documents, and check resource release/idle behavior. Preserve the native picker, safe once-only path/fragment policy, worker loading, error preservation/Retry, embedded scrollers, heading destinations, anchor-aware copying, reading controls and preferences. Stop before Step 8.

### Step 7 session — functionality implemented, memory acceptance in progress

- Implemented: GIO ordinary/atomic-save monitoring, 150 ms burst coalescing, deletion/rename/invalid-UTF-8 recovery and Retry, logical/symlink target and `..` monitoring, dirty-event handling during navigation, an identity/version recheck after installing monitors, request-generation rejection and cancellation of stale presentation. Reading position restores by nearest surviving heading/relative line or proportional fallback. Open queries are recomputed without moving the restored viewport. No permanent polling source or source-document write path was introduced. Stopped before Step 8; Step 7 is **not complete** because the unchanged RSS budget is still exceeded after large-document use.
- Large-document work: cooperative GTK rendering with UTF-8-safe 8 KiB inserts and traversal yields, including code/list/table paths; debounced chunked rendered-text snapshots and cancellable Unicode worker matching with compact ranges. All matches are counted/navigable, while native background tags cover the first 1,000 plus the active match (tooltip explains this). Retained marks reveal matches in unvalidated large buffers without changing selection. Disabled GTK's default undo history in read-only buffers, reduced initial parser container allocations and retire completed models on a worker. Preserve picker, default renderer, worker loading, once-only decoding, embedded scrollers, heading/anchor-aware copy, reading keys and persisted preferences. No dependencies, lockfile, renderer/allocator override, helper process, desktop configuration or user preferences changed.
- Changed: `src/navigation.rs`, `src/document.rs`, `src/state.rs`, `src/native.rs`, `src/document_view.rs`, `src/search.rs`, `src/window.rs`, `src/window/reading.rs`, `src/window/tests.rs`, `src/app.rs` (test hook), new `src/window/reload.rs`, new `src/window/reload_tests.rs`, new `scripts/measure_reload.py`, README, rendering report and this plan. Applicable instruction search again found no repository AGENTS.md; the empty read-only `.git` mount still supplies no usable Git history. Existing user work was inspected before editing and preserved.
- Validation: all required commands pass with `CARGO_HOME="$PWD/.cargo-home"`: `cargo fmt --check`, `cargo check --locked --offline`, strict `cargo clippy --locked --offline --all-targets -- -D warnings`, full desktop `cargo test --locked --offline -- --nocapture` (16 tests), and `cargo build --release --locked --offline`. A sandbox-only full run failed on display/session-bus access as expected; desktop-enabled runs passed. Real notification tests cover ordinary/20-event burst atomic saves, preservation/recovery/Retry, symlink targets/logical replacement, parent paths, heading/proportional viewport restoration, query recomputation, retired-view weak-reference release, monitor cancellation and no leftover debounce timer. Focused Unicode tests cover case-fold expansions and dense results. Five-megabyte prose and 5.25 MB fenced code fixtures verify input/heartbeats during loading, dense/full search counts, stale-query rejection and a final-hit viewport check. Final full-suite maximum main-loop gaps were 18.52 ms for prose/search and 18.09 ms for fenced code; the regression ceiling is 250 ms per main-loop gap, not a hard real-time guarantee.
- Real release validation: reference desktop/default GTK renderer, isolated XDG preferences with existing GTK configuration linked, temporary copy of rendering.md and fixture assets, 30 alternating ordinary/rename-over saves, real Find/Escape/Ctrl+Home during a 5 MB Unicode load, CLI return to the small Unicode fixture, picker cancellation and clean quit (0). Screenshot inspection confirms the typed query and 1-of-1 count; final-hit visibility is verified through the GTK viewport regression, not inferred from that screenshot. No default browser/mail/file handler was actually launched. Old Hyprland window-dispatch syntax failed on the Lua-based compositor; those attempts changed no setting and measurements use actual compositor geometry.
- Final measured memory (MiB / decimal MB): initial 170.88 / 179.18; settled after 30 ordinary/atomic reloads 164.20 / 172.18; normal post-picker cancellation **163.97 / 171.93**. The final run's coarse reload sample maximum was 178.81 MB, with last ten samples 172.11–172.18 MB. Another run sampled a reload transient at 184.85 MB. Five-megabyte search sampled 195.96 / 205.48; after returning to `fixtures/notes/Unicode café.md`, waiting 20 seconds and cancelling the native picker, final retained RSS was **197.00 MiB / 206.57 MB**, with no child processes. Actual normal measurement allocation: 1520 × 956 logical pixels. These are sampled values, not exhaustive peaks or cold-launch/100 KB acceptance. Undo disabling improved an intermediate roughly 234 MB stress sample, but did not resolve retained RSS. Resource-release regressions exclude the tested view/monitor leaks; remaining allocator/toolkit/cache retention has not been attributed sufficiently to claim a cause.
- Remaining Step 7 work: investigate and fix the roughly 207 MB retained RSS after a 5 MB document, and profile/resolve reload transients against the **unchanged 180 MB budget**. Do not introduce allocator FFI/renderer overrides, raise the budget, discard native presentation, truncate content, or mark this step complete just because functional tests pass. `scripts/measure_reload.py` reproduces the stress sequence; preserve all current passing behavior. Pathological unbroken lines, enormous link labels, deep nesting and image/table-heavy 5 MB files are not comprehensively profiled. Full screen reader/high contrast, pointer dragging, actual external handlers and zoom/resize peak profiling remain the previously recorded future checks. Known accessibility-bus refusal and occasional frame-timing warnings persist. Continue Step 7 only and stop before Step 8.

### Step 7 memory acceptance session — complete (2026-11-07)

- Completed retained-RSS acceptance under the unchanged 180 decimal MB settled typical-document budget and investigated reload/picker transients. Step 8 was not started. Observed transient and 5 MB peaks are reported, not claimed below 180 MB.
- Attribution: reproduced 204.62 MB after large prose/return to small; a loading-only comparison and persistent keyboard still retained excess RSS. Diagnostic allocator statistics showed roughly 27 MB free in the main heap among surviving toolkit/font/input allocations. A diagnostic trim reduced 204.24 MB to 178.80 MB. All profiling probes stayed under `/tmp`.
- Decision revision: the prior session prohibited allocator FFI before attributing retention. `src/memory.rs` now calls glibc's thread-safe `malloc_trim(0)` on a GIO worker after large-view disposal or picker close, via a coalesced one-shot 100 ms callback. GTK disposal stays on its thread and no GTK objects cross threads. No permanent application timer, allocator replacement/tuning or renderer override is introduced. Non-GNU libc uses a no-op and has no memory acceptance result.
- Process accounting exposed approximately 11 MB of platform decoder children attached to worker threads, missed by the old main-thread-only child check. Repeated reloads also queued obsolete image work. Queue/short-thread experiments alone did not make platform decoder lifetimes budget-safe. Replaced application pixel decoding with `image` 0.25.10 (default features off, PNG/JPEG/GIF/BMP/PNM only), preserving forced-format validation, 8 MiB reads, 16-million-pixel/dimension limits, animated GIF/vector/remote rejection, filesystem-only once-decoded paths and native GTK MemoryTextures. Configured a decoder-dependent, non-strict 256,000,000-byte allocation limit for high-depth intermediates; file/dimension/pixel validation remains strict. A single-active-job queue uses weak widget/label references and skips retired work. The existing regression proves 50 retired queued images are not decoded; format tests cover all five formats, alpha, scaling and unchanged bytes. P1–P6 remains the PNM policy.
- Search now streams case folding/KMP with query-sized offset history instead of a full-document folded string/map; counts, offsets, expansions and nonoverlap remain covered. Hiding search releases vector capacity. Added weak release checks for large prose views/buffers and code views, and fresh-open top positioning after a scrolled reload. Scroller/CSS/picker size experiments gave no reliable benefit and were removed; the original 720 × 480 picker and existing UI behavior remain.
- Changed: `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/images.rs`, `src/document_view.rs`, new `src/memory.rs`, `src/picker.rs` (close-time reclamation), `src/search.rs`, `src/window/reload_tests.rs`, `scripts/measure_reload.py`, README, this plan, rendering report and two raw JSONL artifacts. All existing locked package versions/checksums are preserved; only the image dependency graph was added/fetched into the workspace Cargo home. Existing work was preserved; no applicable AGENTS.md or usable Git history was available.
- Validation passed with `CARGO_HOME="$PWD/.cargo-home"`: `cargo fmt --check`, `cargo check --locked --offline`, `cargo clippy --locked --offline --all-targets -- -D warnings`, full desktop `cargo test --locked --offline -- --nocapture` (16 tests), and `cargo build --release --locked --offline`. Final heartbeat gaps: prose/search 16.48 ms; code 17.96 ms (ceiling 250 ms). Raster tests also pass without a display. Verified image crate features and preservation of old locked packages. Known accessibility-bus refusal and occasional frame warnings remain.
- Final release measurements: same executable hash, default GTK renderer, fixed 1520 × 956 main window and original 720 × 480 picker, isolated source/preferences and linked existing GTK config. Empty temporary workspace restored afterward. Thirty ordinary/atomic saves, real Find/Escape/Ctrl+Home and picker cancellation, 5,000,082-byte prose and 5,000,104-byte fenced code. Small file after the requested 20 seconds: **163.38 MiB / 171.31 MB** for prose and **167.98 MiB / 176.14 MB** for code. Final settled picker: **159.61 MiB / 167.36 MB** and **166.62 MiB / 174.71 MB**, respectively. Both idle-budget flags true, stable geometry, no children at any checkpoint, quit 0. Raw data: `docs/step7-memory-prose.jsonl`, `docs/step7-memory-code.jsonl`.
- Transients: final main-process reload maxima 180.91/180.45 MB; large-use maxima 223.39/228.80 MB. A diagnostic idle drop correlated with `/dev/nvidiactl` mappings while anonymous memory stayed constant; another trim recovered only 304 KiB. GTK 4.24.1 source confirms 15-second GPU-cache GC, but still-referenced/cached surfaces need not be reclaimed on a particular tick. The script keeps early/peak samples, uses fixed 20-second reload idle and 31-second picker checkpoints, and leaves the original 20-second post-large-file sample unchanged. Fast samples are main-process RSS; checkpoint totals recursively include descendants from every thread. No cache/renderer setting is overridden; no peak RSS cap or exhaustive process-tree peak bound is claimed.
- Reproduce: `python scripts/measure_reload.py --output /tmp/mdview-prose.jsonl` and `python scripts/measure_reload.py --large-kind code --output /tmp/mdview-code.jsonl` (new output paths; desktop/session bus, Hyprland, `wtype`, `gdbus`, no existing mdview, focused app, stable geometry; about two minutes each). Script records exact bytes/MB/MiB, executable hash, process tree, geometry, peaks and fixed idle results; failures exit nonzero.
- Limits remain: normal picker headroom is narrow (179.85 MB); repeat picker and zoom/resize RSS profiling in Step 9. Pathological unbroken lines, huge link labels, deep nesting and image/table-heavy 5 MB files lack comprehensive profiling. Screen reader/high contrast, real pointer dragging/drop and actual external handlers remain Step 8; cold launch/100 KB and zoom/resize peaks remain Step 9. No user source, persistent preferences or desktop configuration changed. **Stop here. Next is Step 8 only when requested.**

### Step 8 accessibility and desktop integration session — complete (2026-11-07)

- Completed: explicit icon-control names, changing search-count/status exposure, live system-font response, high-contrast precedence over saved appearance, opaque selected-match text and unanimated notices/search. Added freedesktop launcher, scalable SVG icon and Markdown MIME/alias declarations. Existing activation/Open/forwarding pipeline handles desktop launches without a new D-Bus service or dependency. Step 9 was not started.
- Changed: `src/app.rs`, `src/window.rs`, `src/window/reading.rs`, `src/reading.rs`, `src/search.rs`, `src/window/tests.rs`, `data/`, new `scripts/check_desktop.py` and `scripts/check_accessibility.py`, README, rendering report, this plan, raw AT-SPI evidence and contrast screenshot. No dependency/lockfile, renderer, memory-budget or persistent desktop/default-association changes. No applicable AGENTS.md or usable Git history exists.
- Validation: formatting, offline locked check, strict all-target Clippy, release build and all 16 desktop tests pass. Extended GTK regression checks 22-point live fonts, native Status roles, reduced-motion revealers and HighContrast with a manual Dark choice; measured document foreground/background contrast **13.77:1**. Existing light/dark, zoom, read-only selection/copy, focus, navigation, picker and reload tests pass. Large-prose/code heartbeat gaps in the full run were 21.62/17.88 ms (250 ms ceiling). Test-only palette lookup retains a scoped deprecated-API allowance, consistent with the existing GTK 4.10 search implementation.
- AT-SPI/real keys: on a dedicated temporary bus, current fixture exposes Unicode prose, ordered table/code children and their full text, image alt/error descriptions and ordered native links. Saved `docs/step8-accessibility.json`. Real guarded `wtype` input passes Find with Café (1 of 1), next, Escape, picker open/cancel, zoom/reset, Tab through embedded content and Enter on a heading link. `scripts/check_accessibility.py` reproduces these checks. The normal desktop accessibility bus still refuses connections; Orca/speech packages are absent. No speech output, announcement delivery, heading rotor or full screen-reader usability is claimed. Tables expose aligned text, not semantic cells.
- Desktop tests: `desktop-file-validate` and temporary MIME/desktop cache generation pass. A disposable prefix with binary/desktop/icon/MIME assets launches via `gtk4-launch`, then `gio launch` opens basic.md and Unicode café.md in two windows through the same desktop-file API used by file managers. Both **native Wayland and X11 through Xwayland** pass, including window identity and multi-file forwarding. Standalone Xorg is unavailable. Actual file-manager GUI gestures are unperformed. `scripts/check_desktop.py` reproduces the tests, removes the prefix and verifies original defaults unchanged (`text/markdown`: `org.kde.kate.desktop`; `text/x-markdown`: unset). Raw desktop results: `docs/step8-desktop.jsonl`. No `xdg-mime default` or system/user installation occurs.
- Visual checks: inspected the app-only large-font/high-contrast screenshot with Find focused, embedded table/code, link labels, image/error descriptions and highlights. Native focus remains visible. Screenshot at `docs/step8-high-contrast.png`. No desktop settings/services were changed; configuration and accessibility services were disposable.
- Limits: normal AT-SPI refusal and occasional frame warnings remain. Physical pointer-drag selection/drop and real browser/mail/other-file handler activation remain manual. No new RSS/cold-start acceptance claim is made; Step 7's GNU-libc idle acceptance remains the reference, with non-GNU libc, transient peaks, picker/zoom/resize headroom and pathological documents still needing the recorded Step 9 coverage. A license must be selected by the user before distribution.
- Next: Step 9 only when requested. Run final release validation/measurements and prepare reproducible installation/uninstallation and the user guide; do not infer a license. Stop here.

### Step 9 release validation and native installation session — complete (2026-11-07)

- Completed: native build/install/uninstall via Makefile, PREFIX/DESTDIR staging, installed short user guide, release functional/performance report, fresh-process 100 KB startup captures, zoom/resize/repeated-picker profiling, final 5 MB prose/code retained-memory runs and runtime network tracing. No app behavior/runtime source changes were needed. No release archive, registry publication or persistent user/system installation occurred. All nine planned steps are now complete with explicit unperformed-check limits.
- User decisions: **MIT** selected explicitly; added LICENSE with 2026 mdview contributors attribution and Cargo `license = "MIT"`. The user also explicitly chose **Accept the measured fresh-process results for startup**, with warm/uncontrolled OS caches disclosed. Cold filesystem-cache behavior remains unperformed; no such result is claimed. GTK default renderer and 180 MB settled typical-document budget remain unchanged.
- Changed: Cargo.toml metadata, new LICENSE/Makefile, new docs/USER_GUIDE.md and docs/RELEASE_VALIDATION.md, new scripts/measure_release.py, scripts/check_network.py and scripts/trace_network.c, extended scripts/check_desktop.py, README, rendering report, this plan and docs/step9-* evidence. No dependency/lockfile change, usable Git history or applicable repository instructions; existing work preserved.
- Required validation: formatting, locked/offline check, strict all-target Clippy, full desktop test suite (16 tests), release build and clean offline release build into initially empty /tmp/mdview-step9-clean-target all pass. Both release executables are 2,842,352 bytes and SHA-256 `78a674f87e5d886c44e62b123528767901ccd19a204f346faa002637719c8877`. Full test heartbeat maxima: 16.92 ms prose/search and 21.60 ms code (250 ms ceiling); contrast 13.77:1. Initial sandbox-only GTK test lacked display access; desktop-access run passes. Known accessibility-bus refusal and occasional frame warnings remain.
- Installation: actual Makefile install/uninstall exercised in a disposable prefix on native Wayland and Xwayland, including binary bytes, launcher/icon/MIME assets, README/guide/MIT license and unrelated-file preservation. Both launcher and multi-file desktop APIs pass. DESTDIR staging/uninstall also passes with spaces/Unicode in the staging path, correct executable mode and caches suppressed. Default associations remain Kate for text/markdown and unset for text/x-markdown. Evidence: docs/step9-desktop.jsonl. No script selects MIME defaults.
- Startup/reading controls: reference Arch x86-64, Core Ultra 9 275HX, roughly 31 GiB RAM, Hyprland 0.57.1, GTK 4.24.1, Rust 1.99.1. Five new release processes, isolated preferences, 100,000-byte UTF-8 fixture, empty temporary workspace and 1520 × 956 allocation. Captured readable content in **827.00–896.04 ms**, median **865.09 ms**; inspected all five captures. Capture timing includes 40 ms frame allowance and screenshot overhead. OS caches were warm/uncontrolled. One-second typical RSS 155.931–156.566 MB; no children at checkpoints. After 150% zoom/reset, float/resize and three picker cycles, fixed 31-second idle is **156.219 MiB / 163.807 MB**, sampled tree peak 165.356 MB. Input/fixture hashes unchanged, zero local HTTP sentinel requests. Evidence: docs/step9-release-isolated.jsonl and five numbered PNGs. Prior diagnostic startup runs are excluded: the first overlapped compilation and failed on obsolete dispatcher syntax; the second used smaller/shared-workspace geometry. Their screenshots were removed; only isolated captures remain. Original workspace/monitor restored.
- Runtime network check: strace unavailable, so added a small Linux x86-64 ptrace syscall helper and reproducible Python orchestration. Unix control passes and Internet-socket creation control fails as expected. All clone/fork/vfork/exec descendants are traced; non-Unix socket families/destinations, unsupported syscall architectures and uninspected io_uring cause failure. Actual release opening of a rendering fixture with local images, blocked remote HTTP/HTTPS images, raw HTML and inactive web/mail links records **only Unix IPC**, root exit 0, no rejected/uninspected operation, unchanged source. Evidence: docs/step9-network.json and .log. No external link is activated, and unrelated desktop daemons are outside this trace's scope.
- Final 5 MB release runs: same binary/default renderer, stable 1520 × 956 geometry and original 720 × 480 picker, disposable sources/preferences and restored workspace. Thirty ordinary/atomic saves, real Find/Escape/Ctrl+Home and picker cancel. Prose/code return-to-small at 20 seconds: **162.05 MiB / 169.93 MB**, **166.92 MiB / 175.03 MB**. Final picker at 31 seconds: **158.54 / 166.24**, **164.54 / 172.54** (MiB / MB). All fixed idle-budget flags true; normal picker is 178.71/178.72 MB. No children at checkpoints, geometry stable, quits 0. Sampled reload peaks 180.07/179.92 MB and large-use peaks 226.59/227.70 MB are recorded separately. Largest sampling gaps 14.17/13.39 ms. Evidence: docs/step9-memory-prose.jsonl and docs/step9-memory-code.jsonl.
- Limits: speech/announcement review unavailable without Orca; physical pointer/file-manager gestures, actual external handlers, standalone Xorg, other libc targets, broader GTK 4.10 platform compatibility and pathological unbroken/link-heavy/deeply nested/image/table-heavy 5 MB profiling remain unverified. These are recorded limits, not checks described as passing. Idle operation retains the existing event-driven design with no permanent polling source. No user document, persistent preferences, desktop configuration or MIME defaults changed.
- Next: no further numbered implementation step. Stop. Future publication/packaging formats, manual verification or new features need a new user request. Reproduce using docs/RELEASE_VALIDATION.md and the README; run graphical scripts sequentially with no existing mdview and new result paths.

### Nameh naming and Git documentation session (2026-11-07)

- User decisions: the app is named **Nameh**; the user initialized Git. Verified initial commit `5a3625d` (`first commit`) and a clean working tree before these edits.
- Completed: updated README, user guide, release report, rendering report and this plan with the chosen product name and current repository status. Historical session records and measurement evidence remain accurate for their original environment; current guidance supersedes the old no-history statements.
- Scope: documentation only. The actual `mdview` command/Cargo package, application ID, desktop/window labels, preferences, assets and license attribution retain their existing identifiers. No implementation rename, new release measurement or commit is claimed.
- Validation: reviewed the documentation diff, checked whitespace and local Markdown links, and confirmed only the five intended Markdown files changed. Application tests were not rerun for prose-only edits; previous validation results remain historical evidence.
- Next: all nine implementation steps remain complete. No further task started; use the available Git repository for subsequent work.

Append future records here. Include completed work, changed files, decisions, checks and results, unresolved issues, and the next step.

## Handover template

At the end of each implementation session, fill in this template and include it as a copyable text block in the response. Use actual results and paths. The handover must be sufficient for a new session to continue without the earlier conversation.

```text
Continue Nameh, the lightweight Linux Markdown viewer, in /home/arash/Documents/markdown.

Git is initialized. Inspect status/diff and preserve unrelated work. The current executable is still mdview; consult the project-name notes before changing identifiers.

Read IMPLEMENTATION_PLAN.md, its latest session record, and any applicable repository instructions first. Inspect the existing code and working tree; preserve unrelated user changes.

Stack: Rust, GTK4 through gtk-rs, pulldown-cmark, and GIO. Native rendering with no browser runtime. Viewer only; never modify source documents. Raw HTML is ignored, remote images are blocked, and unsafe URI schemes are rejected.

Last session: [step number and title; complete or in progress].
Completed: [concrete behavior and changed files].
Decisions: [architectural choices needed to continue].
Validation: [commands and results, plus actual manual checks].
Unresolved: [failures, unperformed checks, or none].

Next task: [step number, title, and specific remaining work].
Run commands: [current build, test, and launch commands].
Complete only this step, update the plan and session records, and finish with another copyable handover for a new session. Stop before the following step.
```
