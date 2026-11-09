/// Why [`Share.share(_:)`] didn't share
/// (`docs/design/45-web-platform.md`, "Web Share").
public enum ShareError implements Error, Equatable {
    /// No share sheet here (desktop Firefox, Chromium on Linux, an insecure
    /// context), or not for this content.
    case unsupported
    /// The user closed the sheet without choosing a target.
    case cancelled
    /// A policy refused, or the user's action was already used.
    case denied
}
