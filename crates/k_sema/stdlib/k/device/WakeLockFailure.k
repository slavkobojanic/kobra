/// Why a [`KeepScreenAwake`] couldn't keep the screen on
/// (`docs/design/45-web-platform.md`, "KeepScreenAwake").
public enum WakeLockFailure implements Error, Equatable {
    /// This browser or context has no screen wake lock (an insecure
    /// context, an engine without the Screen Wake Lock API).
    case unsupported
    /// The browser refused it now: battery saver, a policy, or a page
    /// that was hidden when it asked.
    case unavailable
}
