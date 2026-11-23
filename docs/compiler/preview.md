# `k preview`: the preview server

Design: [57-previews.md](../design/57-previews.md). Code: `crates/k_cli/src/preview.rs` and
`preview/` (the session, the HTTP server, the mounts, the editor protocol, the panel's page), the kit's
preview section (`crates/k_runtime/js/testing/kit.js`: `servePreview`, `framePreview`, the bridge,
`parsePreviewValue`) and the preview bundle ([codegen.md §21.3](codegen.md#213-the-preview-bundle-entrypreviews)).
This page is also the **contract the editor plugins build on**: the VS Code and JetBrains panels start
`k preview --editor`, speak [the editor protocol](#5-the-editor-protocol) and show [the page](#4-urls) in a
webview.

## 1. One process

`k preview [path]` finds the project at or above `path`, analyses it and builds every preview of the
project as one preview bundle (`Entry::Previews(None)`, `Environment::Test`, development profile). It
serves the panel on `127.0.0.1` (`--port`, a free port by default), prints `previews on <url>` (on
standard error with `--editor`, where standard output is the protocol) and opens the URL unless
`--no-open`. `path` below the project root narrows the index to the previews under it.

- **Rebuilds.** The project is watched on disk as `k dev` watches it (the same fingerprint, every 250 ms);
  with `--editor` the editor's unsaved buffers overlay the files. A change rebuilds after 150 ms of quiet.
  Only a program without errors is built: then every page is told to remount (event `build`). Otherwise
  the last good build keeps serving and the first error (of the file changed last, else of the project) is
  the page's badge (event `error`) and the editor's `error` message.
- **Control values** live in the server, per preview, keyed by file and label: the page sets them
  (`POST /api/values`), a rebuild keeps each by parameter name and type
  (`preview::controls::retain`): a renamed or retyped parameter, or an enum value whose case is gone,
  goes back to its default. They survive a reload of the page, so a webview that reloads keeps them.

## 2. Mounts

Every preview the page shows is a **mount**: a `k_server::Server` of its own, on a free loopback port, with
one worker isolate over the preview bundle's server modules, a fresh process (sessions, rate limits) and
an in-memory SQLite database with every migration applied: the hermetic setup of a test
([21-testing.md](../design/21-testing.md)). Its entry runs the kit's `servePreview(server, group,
frame)`:

- the preview's **server container** is built once, as a test's direct container (the group's
  server-side `use` rules, the `k.test` doubles, one session and its virtual clock);
- a document request gets **the frame**, the page that mounts the preview in the browser;
- `POST /_preview/bridge` is [the bridge](#the-bridge);
- every other request (rpcs, `@Load`, endpoint pages) goes to the app's handler as the mount's one
  session, as a test's in-process requests do: the browser's cookies are dropped and the mount's attached,
  and `set-cookie` is dropped from the answers. The front's checks (rpc preconditions, request forgery)
  apply as in production, since the requests are real HTTP.

A slot of the page (the single view, or a grid cell) has at most one mount: mounting again stops the
previous one, and at most 48 run at once (the oldest stops). A rebuild doesn't stop mounts; the page
remounts.

### The frame

The frame is served by the mount, so it has its own origin: its document, styles, theme and storage are
apart from the panel's and from the other frames'. It loads the bundle's client chunks (`/_k/c/…`), the
runtime (`/_k/rt/…`) and the server modules test code imports (`/_k/app/…`) through an import map that
names every `k:` specifier (Chrome doesn't prefix-match keys of an unknown scheme), then calls
`framePreview($groups, $previews, $pages, group)`, which mounts the preview **in the real document** with
fresh browser containers: the same `$mount(values)` `k check` runs, with the kit in *frame mode* (a
rendered page's URL is set with `history.replaceState`; a navigation shows its page inside the layouts,
as in tests).

Its configuration is JSON in the URL's fragment: `{values, params, env, hold}`. `values` are the
control values ([§3](#3-control-values)); `params` the parameters' `{name, control, cases, type}`;
`env` `{scheme: "light"|"dark"|"forced"|null, reducedMotion, width, height, locale, offline}`; `hold`
keeps every request of the preview pending, so each `@Load` stays `loading`.

The environment is applied through the test doubles, so it changes what a test can change:
`TestPreferences` (`prefersDark`, `forcedColors`, `prefersReducedMotion`), with the stylesheets' media
rules on those features forced to match (a rule's condition is rewritten to `all` or `not all`); the
frame's size and `TestViewport.resize`; `TestConnectivity.goOffline()` and failing requests; the
document's `lang` for the locale. Scheme, motion and viewport apply live (`{kpreview: "env", env}` posted
to the frame); offline, locale and holding remount.

The frame posts to its parent `{kpreview: "mounted", title}`, `{kpreview: "failed", message}` (a thrown
setup, a failed mount, a refused guard: the message `k check` reports as K7520, also shown in the
frame), `{kpreview: "invalid", names}` (parameters whose text didn't parse; the default was used) and
`{kpreview: "values", values}` when a `bind` wrote a parameter (`$mount` hands its signals to
`previewParams`), which the page saves and shows without remounting.

### The bridge

A preview's setup runs in the browser, but its server-side `@Inject` fields (`SessionManager`, a fake
bound where server code uses it, a repository) live in the mount's isolate. The browser's
`$k.$test.server()` is a proxy over a **synchronous** bridge (`XMLHttpRequest` to `/_preview/bridge`):
reading a member asks the server for it, calling one calls it there and waits for its promise. Values
cross as tagged JSON (`toWire`/`fromWire`): primitives, `undefined`, non-finite numbers, `BigInt`, lists,
`Uint8Array`, maps, sets, plain objects (structs, payload cases), payload-free enum cases (one value per
tag on each side), `Instant`, `Secret` and `Money`; anything else (a service, a function) crosses as a
handle, a proxy on the other side. A browser object that is none of these can't be passed to the
server (a panic says so). Each answer carries the session the server container left, which the browser
mirrors, so a page's guards see a setup's sign-in.

## 3. Control values

A parameter's control, as `/api/state` and the frame write it: `{"kind": "toggle"}` (`Bool`),
`"integer"` (`Int`), `"number"` (`Double`), `"text"` (`String`), `{"kind": "choice", "cases": [...]}` (a
payload-free enum, cases in order), `{"kind": "parsed", "type": "k.core.Money"}` (a type with a literal
syntax). Its value is the JSON of what the control holds: a boolean, a number (a whole one for `Int`), a
string, **the case's name** for an enum (the frame passes `$mount` the case value, `unitCase(index)`),
and **the text** for a parsed type, which the frame parses with the type's parser
(`parsePreviewValue`): `Money` literals (`24.00.usd`, `10.eur`), `Duration` literals (`5.minutes`,
`1.5.hours`), the stdlib's `parse` (`Date`, `Url`, `Uuid`, `EmailAddress`, `Decimal`, `TimeOfDay`,
`TimeZone`) and a project type's `static func parse(_:)` (`$previews[i].parse[name]`). Text that doesn't
parse is reported back (`invalid`) and the field is outlined. `null` is the default. The defaults shown
are the literal defaults; a computed one shows its source as the placeholder.

## 4. URLs

| URL | What |
|---|---|
| `GET /` | the index: every preview (under `path`), grouped by file, linking to their pages |
| `GET /p/<file>/<index>` | the panel on the `index`th preview (source order) of `file` (a project path). Query: `mode=grid` (every preview of the file), `embed=1` (no link to the index or file name, for a webview), `theme=light\|dark` (the panel's own colours; the system's by default) |
| `GET /api/state` | `{name, under, build, error, selected, files: [{path, previews: [{index, group, label, line, params: [{name, type, control, default, defaultSource}], values}]}]}` |
| `GET /api/events` | server-sent events, one JSON object each: `{type: "build", build}`, `{type: "error", error: {file, message, line, column, count}}`, `{type: "select", file, index}` |
| `POST /api/values` | `{group, values: {name: value\|null}, reset?}`: sets control values (`reset` restores every default first); answers `{values}`, or `400 {errors: {name: message}}` |
| `POST /api/mount` | `{group, slot}`: a fresh mount for a slot of the page; answers `{url, build}`, the frame's URL, to which the page appends its configuration as the fragment |

An embedding window can also drive the page with `postMessage`: `{kpreview: "select", file, index}`
switches the preview, `{kpreview: "env", env: {scheme, motion, viewport, width, height, locale, offline,
hold}}` sets the environment bar (`viewport` is `fit`, `390x844`, `820x1180`, `1280x800` or `custom`).
The editors normally don't need either: `select` over the protocol moves every open page.

## 5. The editor protocol

`k preview --editor [--no-open] [path]`: JSON objects, one per line, on standard input (editor to `k`)
and standard output (`k` to editor). Offsets are **UTF-16 code units** of the buffer's text, as editors
count them; URIs are `file://` URIs of the project's files. Logs go to standard error. The process stops
on `shutdown` or when standard input closes.

The editor talks to `k preview` directly rather than through `k lsp`: it already has the buffers, and
the language server stays free of the preview server's lifetime (decided in
[57-previews.md](../design/57-previews.md#built-so-far)).

Editor to `k`:

| Message | Fields | Effect |
|---|---|---|
| `didOpen`, `didChange` | `uri`, `text` (the whole buffer) | overlays the file with the unsaved text; a rebuild follows after 150 ms of quiet, built only when the program type-checks |
| `didClose` | `uri` | drops the overlay: the file on disk counts again |
| `select` | `uri`, `offset` | the cursor moved: when it is inside a `preview` declaration, every page switches to it (event `select`) |
| `shutdown` | | stops the server |

`k` to editor:

| Message | Fields | When |
|---|---|---|
| `ready` | `url` (`http://127.0.0.1:<port>/`) | first, once the page is served (the first build follows) |
| `built` | `build` (a number) | after each good build; clears an earlier `error` |
| `previews` | `uri`, `list: [{index, label, line, start, end, path}]` | after each good build, for every file with previews and every file the editor sent or selected in (an empty list for a file without). `start`/`end` are the keyword's UTF-16 offsets, `line` is 1-based, `path` the page's path (`/p/src/shop/Counter.k/0`; append `?embed=1`) |
| `error` | `uri` (or `null`), `message`, `line`, `column` (1-based, or `null`) | when the sources have an error: the page keeps the last good build under a badge with the same message; also for a message `k` couldn't read |

### The VS Code panel

`editors/vscode/preview/` is this protocol's first client: `client.js` (the process, restarts with
backoff, the buffers resent after each `ready`, each file's last `previews`, the error) and `index.js`
(the webview on `<url>p/<file>/<index>?embed=1&theme=light|dark[&mode=grid]`, reached through
`asExternalUri`, following the active editor and posting `{kpreview: "select"}` into the page). Decisions
are in [57-previews.md](../design/57-previews.md#built-so-far) (round 3), the user's view in
`editors/vscode/README.md`.

## 6. Tests

`crates/k_cli/tests/preview.rs`: the editor protocol on the `previewing` fixture (index, page, values
kept across a rename, a broken edit keeping the last build under the badge, `select`, `didClose`,
`shutdown`), and in headless Chrome the panel's toggle changing what the frame renders. Unit tests in
`preview::controls` (control JSON, defaults, checking values, retention), `preview::editor` (messages,
UTF-16 offsets) and `preview::http`. The VS Code client's tests are `node --test` in `editors/vscode` (no npm
install): the client against a fake `k preview` (`test/fake-k`) and, when a `k` is built (`KOBRA_K`, else
`target/debug/k`), against the real `k preview --editor` on the `previewing` fixture; the panel on a stub
of the `vscode` module. `crates/k_lsp/tests/editors.rs` checks that the extension registers the CodeLens's
command.

## 7. Limits

- A mount's server-side `use` rules apply to the mount's requests; channels (`@Channel` rpcs) reach the
  mount's server, but not through the group's container yet.
- The frame has no `TestClock` stepping and no mapping of elements back to source
  ([57-previews.md](../design/57-previews.md#the-panel)).
- A refused guard shows `k check`'s message rather than the sign-in page (57-previews.md, "Built so far").
- Locale sets the document's `lang` only: the runtime formats in `en` regardless.
