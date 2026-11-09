import shell.AppLayout

/// Call: a 1:1 video call between two browsers
/// (`docs/design/46-media-and-rtc.md`, "Example apps").
///
/// - **The default layout** wraps every page in `AppLayout`.
/// - **The sign-in page** is where the `Authenticated` guard sends visitors
///   without a session.
///
/// There are no `use … for …` rules. `PeerConnections` and `MediaDevices`
/// are client-only platform interfaces with one production implementation
/// each, and `k test` replaces them with `TestPeerConnections` and
/// `TestMediaDevices`. `IceServers` reads `TurnConfig` (`K_TURN_URLS`,
/// `K_TURN_SECRET`): unset in development, where two tabs on one machine
/// need no server at all. There is no database: a room is an unguessable
/// link, and sessions live in the server's memory.
@Layout(AppLayout)
@SignInPage(.signIn)
public app Call {
    metadata {
        manifest(name: "Call", shortName: "Call")
        themeColor(.primary)
        locale(.english)
    }
}
