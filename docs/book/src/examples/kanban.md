# Kanban: drag and drop in the browser

`examples/kanban` in the repository is a complete app that runs entirely in the browser: boards of
columns and cards that you drag between columns, edit, import and export. It builds to static files
only and ships in an nginx image. Its README walks through every file; this page shows how the parts
of K it uses fit together.

| Feature | Where | Chapter |
|---|---|---|
| Cards dragged within and between columns, with the mouse, touch or the keyboard | `pages/ColumnView.k` | [Drag and drop](../apps/drag-and-drop.md) |
| N, E, ? and Escape on the board page | `pages/BoardPage.k` | [Keyboard shortcuts](../apps/keyboard-shortcuts.md) |
| Editing a card with validation | `pages/CardEditor.k`, `board/CardDraft.k` | [Forms and validation](../apps/forms.md) |
| The progress chart | `pages/ProgressChart.k` | [Charts](../apps/charts.md) |
| Import and export as JSON | `board/BoardTransfer.k` | [Importing and exporting files](../apps/files.md) |
| Boards kept across reloads | `board/BoardService.k`, `storage.lock` | [Local storage](../apps/local-storage.md) |
| Light, dark and high-contrast looks | `ui/KanbanTheme.k`, `ui/ThemePicker.k` | [Styles and themes](../apps/styles.md) |
| Progress stamped by the injected clock | `board/BoardService.k` | [Time and timers](../apps/time.md) |
| Static files in an nginx image | `Dockerfile`, `nginx.conf` | [Deployment](../apps/deployment.md#static-hosting) |

## One source of truth

`BoardService` owns every board. Pages read `boards.boards` and `boards.board(id:)`, and call its
methods; nothing else changes a board. A drop is no exception: a column's `ReorderableList` fires
`moved`, and the service applies the `ListMove`:

```k
// examples/kanban/src/pages/ColumnView.k
ReorderableList(kind: cardDrag, id: column.id.text, items: column.cards, key: { card => card.id.text }, label: column.title)
    .row { card =>
        if editing == card.id {
            CardEditor(board: board, card: card)
                .on(closed: { closed.emit() })
        } else {
            CardView(card: card, isDone: isDone)
                .on(edit: { edit.emit(card.id) })
        }
    }
    .itemLabel { card => card.title }
    .on(moved: { move => boards.moveCard(board: board, move: move) })
```

`moveCard` checks the move against the board (a column that is gone, a position past the end) before
it removes and inserts, so a stale drop can't lose or duplicate a card. Every change is saved to local
storage, and a change of the done or total count adds a point to the board's history, stamped with
`clock.now`; the chart plots that history.

## Running it

```sh
k test examples/kanban          # 41 tests: services, pages, drags, shortcuts, files
k dev examples/kanban           # the app on http://localhost:8080
k build examples/kanban         # dist/public only: index.html and hashed assets
docker compose -f examples/kanban/docker-compose.yml up --build   # nginx on :8080
```

The tests never touch a real browser's storage or file picker: `k test` installs `MemoryStorage`,
`TestFiles` and `TestClock`, and tests drag with `drag(to:)` and the keyboard with `press`.
