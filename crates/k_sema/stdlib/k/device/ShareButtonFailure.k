/// Why a [`ShareButton`] neither shared nor copied
/// (`docs/design/45-web-platform.md`, "ShareButton"). Closing the share
/// sheet isn't a failure.
public enum ShareButtonFailure implements Error, Equatable {
    /// The browser refused the share or the copy: a policy, or the user's
    /// action was already used.
    case denied
    /// The browser can neither share nor copy here (an insecure context).
    case unsupported
}
