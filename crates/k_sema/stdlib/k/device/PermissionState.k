/// Whether the app may use a permissioned browser feature
/// (`docs/design/45-web-platform.md`, "Permissions"). A permissioned
/// interface exposes it as a reactive `permission` member; only its
/// `request…` member shows a prompt, and returns the new state.
public enum PermissionState implements Equatable {
    /// The next request will ask the user, or the browser can't say.
    case prompt
    /// The user allowed it.
    case granted
    /// The user, a browser setting or a policy refused it; only the
    /// browser's site settings can change it now.
    case denied
}
