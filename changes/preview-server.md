---
kind: preview
design: 57
book: apps/previews.md, reference/cli.md
---
`k preview [path]` serves a project's previews in the browser: an index by file, and a panel per preview with a switcher, controls for its parameters (toggles, enum dropdowns, number steppers, text fields, and parsed fields for `Money`, `Duration` and `parse` types), an environment bar (colours, reduced motion, viewport presets, locale, offline, hold loading) and a grid of every preview of a file. Each preview mounts in a real browser frame with fresh hermetic containers, and every save remounts it, keeping the control values; a build with errors keeps the last good one under a badge. `--editor` speaks a JSON-lines protocol for the editor panels, and `k lsp` adds a **▶ Preview** CodeLens on each `preview`.
