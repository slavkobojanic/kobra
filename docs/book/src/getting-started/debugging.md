# Debugging

You debug K as K. Breakpoints go in `.k` files, the debugger stops on `.k` lines, stack traces name
`.k` lines, and the values you inspect read as K values. This works in the developer tools of Chrome,
Edge, Firefox and Safari, and in VS Code, with nothing to install: `k dev` writes a source map for every
file it generates, and those maps lead from the generated JavaScript back to your code.

Take this component:

```k
public component Counter {
    @State var count: Int = 0

    view {
        button { "Add one ({count})" }
            .on(click: {
                let next = count + 1
                count = next
            })
    }
}
```

Set a breakpoint on `count = next`, click the button, and the browser stops there with `next`, `count`
and the component's other members in the Scope pane, by their K names.

## In the browser

Start the app with `k dev` and open it. Then:

- **Chrome and Edge.** Open DevTools (F12), then the **Sources** panel. Your files are under
  `_k/src` in the page tree, laid out as in your project (`src/counter/Counter.k`); press Ctrl+P (Cmd+P
  on a Mac) and type a file name to open one. Click a line number to set a breakpoint.
- **Firefox.** Open the developer tools, then **Debugger**. The `.k` files are under `_k/src`.
- **Safari.** Turn on "Show features for web developers" in the Advanced settings, open the Web
  Inspector, then **Sources**.

What you can set breakpoints on: any statement in a function, a handler (`.on(click: { … })`), an
`async` function (it stops again after each `await` you step over), a `match` arm, a `@Derived` or `@Load`
initializer (it stops each time the value is recomputed or loaded), and a view binding such as
`"Add one ({count})"` (it stops each time that text re-renders).

**Stepping stays in your code.** The K runtime is on the source maps' ignore list, so Step into goes
from your code to your code, and stack traces show your functions without the runtime between them.
That holds for `await` too: the async stack trace of a stopped `async` function shows the `.k` lines that
led to it, across rpc calls.

**Errors name `.k` lines.** An error thrown in the browser shows in the Console with its stack trace
mapped to your files. Click a line to jump to it.

### Seeing K values

Values in the Scope pane, in hovers and in the Console are JavaScript values underneath. Turn on custom
formatters to see them as K values: in Chrome's DevTools settings (the gear icon, Preferences, Console),
check **Enable custom formatters**. With it on, while you develop:

| Value | Shown as |
|---|---|
| `@State var count: Int` | `@State 3`, expandable when it holds a struct or a list |
| `@Derived let label: String` | `@Derived "Clicked 3 times"` |
| `nil` | `nil` |
| a payload enum case | `.outOfStock(available: 3)` |
| a struct | `struct { title: "Milk", done: false }`, expandable by field |
| `Money`, `Decimal` | `Money 12.50 USD`, `Decimal 0.075` |
| `Date` | `Date 2026-11-09T10:00:00.000Z` |
| `List`, `Map`, `Set` | `List(3) [1, 2, 3]`, `Map(2)`, `Set(4)`, expandable |

Inspecting a `@State` value this way never subscribes anything to it, so it doesn't change what
re-renders. Payload-free enum cases show as their position (`.#0`). The formatters are only in the
development build; the code `k build` ships doesn't contain them.

## The server

Code that runs on the server (rpc handlers, `@Load` code during server rendering, endpoint pages,
background jobs) is debugged the same way. Start the app with its debugger port open:

```bash
k dev --inspect
```

`k dev` prints `inspector on ws://127.0.0.1:9229/ws/k`. Use `--inspect 9230` for another port. While a
debugger is attached, the server runs one isolate, so every request goes through it.

- **Chrome.** Open `chrome://inspect`, choose **Configure…**, add `localhost:9229`, then click
  **inspect** under "K server". The `.k` files are under `_k/src` in the Sources panel.
- **VS Code.** Use the attach configuration below.

Without a debugger attached, you still get `.k` lines:

- **Logged stack traces** name `.k` lines, with the JavaScript location after them:
  `at greet (src/home/GreeterApi.k:5:9) [k:/app/home.js:40:22]`.
- **The error page.** While you develop, a request that fails with a panic shows its stack trace on the
  500 page, and a failed rpc carries it in its error message. A production server shows neither.

## VS Code

VS Code's built-in JavaScript debugger needs no extension. Copy `editors/vscode/launch.json` and
`editors/vscode/tasks.json` from the K repository into your project's `.vscode/` folder. They hold three
configurations:

- **K: debug in Chrome** starts `k dev --inspect` and opens the app in Chrome, with breakpoints in your
  `.k` files.
- **K: attach to the server (k dev --inspect)** attaches to the server.
- **K: browser and server** does both, so one session stops on both sides of an rpc.

The part that makes `.k` breakpoints work is the path mapping. The maps call your files
`/_k/src/<path>` in the browser and `k:/_k/src/<path>` on the server:

```json
{
  "name": "K: attach to the server (k dev --inspect)",
  "type": "node",
  "request": "attach",
  "port": 9229,
  "sourceMaps": true,
  "sourceMapPathOverrides": { "k:/_k/src/*": "${workspaceFolder}/*" },
  "skipFiles": ["k:/runtime/**"],
  "restart": true
}
```

`${workspaceFolder}` must be the folder with `k.toml`. In a workspace of several apps, point it at the
app's folder. `"restart": true` reattaches after `k dev` restarts the server on a change.

## Release builds

`k run --release` serves the client `k build` ships, minified. Its source maps lead from the minified
code straight to your `.k` lines and names, so you can debug the shipped code too. They don't include
your source text, which `k run` serves next to the app.

In production, the maps are written next to the files in `dist/public/assets`, but the server built
with the app serves them only when `K_PUBLIC_SOURCE_MAPS` is `true`. A source map reveals how your code
is structured and what your files are called, so publishing them is your choice. On a static host,
delete the `*.map` files before you upload if you don't want them public. The server's own stack traces
are mapped either way: its maps are inside the server and never sent to browsers.
