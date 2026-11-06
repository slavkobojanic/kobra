/// A small todo list that keeps everything in the browser.
///
/// Pages, services and the theme are found by the compiler from their
/// declarations, so this file only holds what is genuinely app-wide: how
/// pages are rendered, and the document entries shared by every page.
///
/// `@Rendering(.client)` turns server rendering off. The todos live in the
/// browser's local storage, which the server can't read, so a server-rendered
/// page would always show an empty list and then jump to the real one. The
/// app has no server-only code either, so `k build` emits static files only.
///
/// There are no `use … for …` rules. `k.storage.LocalStorage` has exactly
/// one production implementation, the stdlib's `k.storage.BrowserStorage`,
/// so it is injected automatically. In tests, `k test` installs
/// `k.test.MemoryStorage` in its place by default, so no test touches real
/// browser storage.
@Rendering(.client)
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
