/// Why [`Clipboard.write(text:)`] didn't copy
/// (`docs/design/45-web-platform.md`, "Clipboard").
public enum ClipboardWriteError implements Error, Equatable {
    /// This browser or context has no clipboard ([`Clipboard.isSupported`]
    /// is `false`).
    case unsupported
    /// The browser refused: a policy, or no user's action.
    case denied
}
