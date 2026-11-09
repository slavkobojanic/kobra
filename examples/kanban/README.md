# Kanban

Kanban boards written entirely in K. Create boards with To do, Doing and Done columns, add and edit
cards, and move them between columns by dragging them with a mouse or a finger, or with the keyboard.
A chart shows each board's progress over time. Boards can be exported as JSON files and imported again,
in this browser or another one. Everything is kept in the browser's local storage, and there are light,
dark and high-contrast themes.

The app renders on the client and has no server-only code, so `k build` produces **static files
only**, `dist/public`. The `Dockerfile` serves them with nginx; no account on any service is needed to
run or deploy it.

## Running it

From this directory:

```bash
k dev
```

Then open <http://localhost:8080>. There is nothing to set up: boards live in the browser's storage, so
there is no database and no configuration. `k dev` rebuilds and restarts on every change (`--port` picks
another port).

The other commands:

| Command | What it does |
|---|---|
| `k test` | Runs the 41 tests of `*.test.k`: the board service, import and export, the pages with their drags, shortcuts and forms. |
| `k check` | Type checks, checks `storage.lock` and runs the tests, as CI and the image build do (`--no-tests` skips them). |
| `k build` | Writes `dist/public`: `index.html` and the hashed assets (minified, each script with its source map, about 30 KB gzip in all). There is no `dist/app`. |
| `docker compose up --build` | Checks, builds and serves the app with nginx on <http://localhost:8080>. |

The image is two stages: the `k` toolchain, built from this repository's source (or a published
toolchain image passed as `--build-arg K_TOOLCHAIN=...`), runs `k check` (tests included) and `k build`, then
`nginxinc/nginx-unprivileged` serves `dist/public` as a non-root user on port 8080, read-only, with a
health check. [`nginx.conf`](nginx.conf) answers `index.html` for every URL that isn't a file, so a link
to `/boards/<id>` reaches the client router; the document is sent with `Cache-Control: no-cache`, so a
deploy is seen on the next load, and the hashed files below `/assets/` with
`Cache-Control: public, max-age=31536000, immutable`, a missing one being a 404. Its
Content-Security-Policy allows no inline script except the document's pre-paint theme snippet, by its
hash, which puts the remembered theme on `<html>` before the first paint so a reload keeps it
(`crates/k_server/tests/theme_snippet.rs` checks the hash). The examples' tests
serve the build the same way ([`crates/k_cli/tests/support/static_host.rs`](../../crates/k_cli/tests/support/static_host.rs)).

## Using it

- **Move a card with the mouse:** drag it onto another card, which it takes the place of, or anywhere
  else in a column (its heading, the space below its cards, an empty column), which puts it at the
  bottom. Clicking a card's Edit button never starts a drag.
- **Add or edit a card:** "Add a card" (or **N**) opens the form with the focus in its title field, and
  Edit opens the editor the same way (`.autofocus`), so you can type at once.
- **Move a card on a touch screen:** press and hold it for a moment, then drag. A quick swipe scrolls.
- **Move a card with the keyboard:** Tab to the card and press **Space** or **Enter** to pick it up.
  **ArrowUp** and **ArrowDown** move it within its column, **ArrowLeft** and **ArrowRight** to the
  previous or next column. **Space** or **Enter** drops it; **Escape** puts it back. Each step is
  announced to screen readers ("Write tests: Position 1 of 2 in Doing").
- **Shortcuts on a board:** **N** adds a card to the first column, **E** exports the board, **?** shows
  the list of shortcuts, **Escape** closes the editor. They never fire while you type in a field.
- **Export** downloads `<board-name>.json`. **Import board…** on the home page reads one back: a board
  with the same id replaces this browser's copy, any other is added.

## Structure

```
kanban/
├── k.toml                      project manifest
├── App.k                       client rendering and app-wide metadata
├── storage.lock                the storage namespace, the key and the stored shapes
├── Dockerfile                  toolchain stage, then nginx
├── nginx.conf                  fallback to index.html; caching of hashed assets
├── docker-compose.yml
└── src/
    ├── board/                  module `board`: the domain and its services
    │   ├── Board.k, Column.k, Card.k, ProgressPoint.k, Priority.k
    │   ├── BoardId.k, ColumnId.k, CardId.k
    │   ├── CardDraft.k         the card form's struct, with its constraints
    │   ├── cardDrag.k          the DragKind<Card> every column's list shares
    │   ├── BoardService.k      + BoardService.test.k: every board, saved on each change
    │   ├── BoardTransfer.k     + BoardTransfer.test.k: export and import as JSON
    │   ├── BoardFile.k         the exported file: a format version and the board
    │   └── TitleError.k, StorageProblem.k, TransferProblem.k
    ├── pages/                  module `pages`
    │   ├── Home.k              + Home.test.k: the boards, creating and importing
    │   ├── BoardPage.k         + BoardPage.test.k: one board at /boards/<id>
    │   ├── ColumnView.k        a column: its ReorderableList of cards
    │   ├── CardView.k, CardEditor.k, NewCardForm.k
    │   ├── ProgressChart.k     the LineChart of the board's history
    │   └── ShortcutList.k
    ├── ui/                     module `ui`: the theme and app-agnostic parts
    │   ├── KanbanTheme.k       tokens, `variant dark` and `variant contrast`
    │   ├── ThemePicker.k
    │   └── Notice.k
    └── testing/
        └── Samples.k           fixtures: Board.sample, Board.seed(in:cards:)
```

## How it works

**One source of truth.** `BoardService` holds every board in `@State`, and only its methods change
them. Each change is written to local storage under the typed key `StorageKey<List<Board>>`, whose name
and value shape are recorded in `storage.lock`, so a change that would make saved boards unreadable
fails `k check`. A storage failure never stops the app: the change applies in memory and a notice says
what isn't being saved.

**Drag and drop never moves anything by itself** ([41-drag-and-drop.md](../../docs/design/41-drag-and-drop.md)).
Every column is a `ReorderableList` of the kind `cardDrag`, a `DragKind<Card>`, so cards move between
columns and nothing else can be dropped on one. A drop fires `moved` with a `ListMove<Card>`, and
`BoardService.moveCard` removes the card from where it was and inserts it where it was dropped,
ignoring a move that no longer matches the board. Both columns redraw from the new state.

**Keyboard shortcuts are components** ([43-keyboard-shortcuts.md](../../docs/design/43-keyboard-shortcuts.md)).
`BoardPage` mounts one `KeyboardShortcut` per key, so they exist only on the board page, and each has
a button that does the same.

**Forms validate on both ends of typing.** `CardEditor` and `NewCardForm` bind a `@Form` of
`CardDraft`, whose `@NotBlank` and `@Length` constraints are checked as the user types and again by
`form.handle(_:)`, which runs the save only for a valid draft
([15-forms-and-validation.md](../../docs/design/15-forms-and-validation.md)).

**A card's link is one `CopyButton`** ([45-web-platform.md](../../docs/design/45-web-platform.md#copybutton)).
The editor's "Copy link" copies the board page's address with `?card=<id>`, made absolute by
`ShareContent.page`, and says "Copied" for two seconds. `BoardPage`'s optional `card` parameter opens
that card's editor, so the link works in another tab of the same browser, where the boards are.

**Progress is history, stamped by the injected clock.** Whenever a board's done or total count changes,
`BoardService` appends a `ProgressPoint` at `clock.now`; `ProgressChart` draws the history with
`k.charts`' `LineChart`, colored by theme tokens, with the accessible data table every chart has.

**Files go through `Files`** ([42-files.md](../../docs/design/42-files.md)). `BoardTransfer` saves a
`BoardFile` (a format version and the board) as a JSON download and reads one back with
`files.open(accepting: [.json])`, called from the Import button's click so the browser shows the
picker. Closing the picker is `.cancelled`, which isn't a problem; a file that doesn't decode as a
`BoardFile`, or of a newer format, is.

## The tests

| File | What it covers |
|---|---|
| `BoardService.test.k` | Creating boards and their default columns, title rules, adding cards, moves within and between columns, stale moves ignored, editing and removing, progress history on the `TestClock`, saving and restoring, unreadable and full storage. |
| `BoardTransfer.test.k` | Export file name and content, an export imported back, a board from elsewhere, a closed picker, a file that isn't a board, a newer format, too large and unreadable files, blocked downloads. |
| `BoardPage.test.k` | Columns as named lists; moving a card with Space, ArrowRight and Space; reordering with ArrowDown; Escape cancelling; mouse drags onto a column and onto a card; the editor's validation and save; "Copy link" putting the card's link on the `TestClipboard`, and the link opening the editor; deleting; the N, E and ? shortcuts and a shortcut key typed into a field; an unknown board. |
| `Home.test.k` | The empty state, creating a board (which opens it), a blank name, the board list with progress, importing (which opens the board), an import problem dismissed, the theme buttons, boards kept across visits. |

`k test` installs `MemoryStorage`, `TestFiles`, `TestClipboard` and `TestClock` in place of the
browser's storage, file picker, clipboard and clock, so no test touches a real browser. Tests drag the way users do:
`item.drag(to: screen.list(named: "Done"))` for the mouse, `press(.space)` and the arrow keys for the
keyboard, `press(.character("n"))` for a shortcut.

The repository's own suites also build the app and check that it is static files only, serve
`dist/public` like the nginx image and request a deep link, and drive the shipped build in a simulated
browser tab: creating a board, adding cards with the N shortcut, moving one with the keyboard and one
with the mouse, and finding both moves in local storage.
