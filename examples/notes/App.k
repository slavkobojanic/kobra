import shell.AppLayout

/// K Notes: a full-stack notes app with accounts, live updates across tabs,
/// and conflict-safe editing, on SQLite.
///
/// Pages, services, protocol implementations, repositories and the theme are
/// found by the compiler from their declarations, so this file holds only
/// what is genuinely app-wide:
///
/// - **The default layout**, which wraps every page in `AppLayout`.
/// - **The sign-in page**, where the `Authenticated` guard sends visitors
///   without a session.
/// - **The document entries** shared by every page.
///
/// There are no `use … for …` rules: every injected interface has exactly
/// one implementation, and tests get the stdlib's test defaults, such as
/// `TestClock` for `Clock`. `Database` is the stdlib's own. It connects with
/// `DatabaseConfig` to `DATABASE_URL` (a SQLite file in development, see
/// `.env.example`), and to a fresh in-memory database for each `k test` run.
@Layout(AppLayout)
@SignInPage(.signIn)
public app KNotes {
    metadata {
        manifest(name: "K Notes", shortName: "Notes")
        themeColor(.primary)
        locale(.english)
    }
}
