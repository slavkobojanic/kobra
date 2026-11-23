---
kind: tooling
design: 57
book: apps/previews.md, getting-started/editor-setup.md
---
The VS Code extension shows previews in a **Kobra Preview** panel beside the editor: it runs `k preview --editor` for each project that enables previews, sends it the unsaved buffers, opens the panel when a `.k` file with previews becomes the active editor (setting `kobra.preview.autoOpen`), follows the active file and the cursor, and offers the **▶ Preview** CodeLens, an editor-title button, **Kobra: Open Preview** and a grid toggle. A preview error shows in the status bar until the next good build; clicking it goes to the error.
