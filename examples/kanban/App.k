/// Kanban boards that live in the browser.
///
/// `@Rendering(.client)` turns server rendering off: the boards are in the
/// browser's local storage, which a server can't read. The app has no
/// server-only code either (no rpc, no configuration, no entity), so
/// `k build` emits static files only, `dist/public`, which the nginx image
/// of the `Dockerfile` serves.
///
/// No `use … for …` rules are needed. `LocalStorage` and `Files` each have
/// one production implementation in the stdlib (`BrowserStorage`,
/// `BrowserFiles`), and `k test` installs `MemoryStorage`, `TestFiles` and
/// `TestClock` in their place.
@Rendering(.client)
public app Kanban {
    metadata {
        title("Kanban")
        description("Kanban boards that live in your browser.")
        manifest(name: "Kanban", shortName: "Kanban")
        themeColor(.primary)
        locale(.english)
    }
}
