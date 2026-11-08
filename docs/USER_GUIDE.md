# Reading with Nameh

Nameh currently runs as `mdview` and appears as **mdview** in the desktop launcher
and window titles. Its command and preference paths below use that existing name.

Open a local Markdown document with **Open** (Ctrl+O), drop one file on a window,
or run `mdview "path/to/notes.md"`. Each window shows one document. Opening another
file replaces it after loading succeeds. **New Window** (Ctrl+N) opens a separate
window. Ctrl+W closes a window; Ctrl+Q quits.

The file picker accepts a path as well as folder navigation. Ctrl+L focuses its
path field, Enter opens the selection, Up visits the parent folder, and Escape
cancels. Enable **All files** to show files outside the Markdown filter.

Select text and press Ctrl+C to copy. Tables copy as aligned text. A selection in
the main document includes the complete contents of crossed tables and code
blocks; select inside a block to copy only part of it. Scroll wide tables/code
inside their own frames. Arrow keys, Page Up/Down and Ctrl+Home/End support reading.

Use Ctrl+F to search, Ctrl+G or Enter for the next match, and Ctrl+Shift+G for the
previous match. Escape closes Find. Search includes tables, code and image
descriptions. It ignores case and wraps at the end.

Ctrl++ and Ctrl+- change text size; Ctrl+0 resets it. The reading menu offers
System, Light and Dark appearance. System high contrast takes precedence.
Appearance, zoom and window size are remembered. Preferences are stored in
`$XDG_CONFIG_HOME/mdview/preferences.ini`, normally `~/.config/mdview/preferences.ini`.

Heading links jump within the document. Markdown file links open in the same
window. Activated web, email and other readable file links use your desktop's
default application. Unsafe links and executable files are blocked. Remote
images and raw HTML are not displayed; supported local raster images load from
the document's folder. No document edits or saves are performed.

External saves reload automatically and preserve the reading position where
possible. If a file disappears or becomes unreadable, the last readable version
stays visible. Use **Retry** after correcting the problem. A missing heading in an
otherwise readable destination opens that document and reports the missing heading.

Tables expose text rather than screen-reader cell navigation. Continuous partial
selection from inside an embedded block into the main text is unavailable.
Speech-reader behavior and physical drag gestures have not yet been fully reviewed.
