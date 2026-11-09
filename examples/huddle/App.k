import shell.AppLayout

/// Huddle: a small team chat on channels (`docs/design/44-channels.md`).
///
/// - **The default layout** wraps every page in `AppLayout`.
/// - **The sign-in page** is where the `Authenticated` guard sends visitors
///   without a session.
///
/// `Database` connects with `DatabaseConfig` to `DATABASE_URL` (a SQLite file
/// in development), and to a fresh in-memory database for each `k test`
/// run. Channels need no configuration: they share one WebSocket per tab at
/// `/_k/channels`, and their limits are the stdlib's `ChannelConfig`.
@Layout(AppLayout)
@SignInPage(.signIn)
public app Huddle {
    metadata {
        manifest(name: "Huddle", shortName: "Huddle")
        themeColor(.primary)
        locale(.english)
    }
}
