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
/// There are no `use … for …` rules. `LocalStorage` has exactly one
/// implementation outside tests, the stdlib's browser storage, so it is
/// injected automatically. Tests replace it per test group with
/// `MemoryStorage`.
@Rendering(.client)
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
