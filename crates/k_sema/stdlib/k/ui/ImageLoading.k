/// The `.loading(...)` attribute of `image`.
public enum ImageLoading {
    /// Load at once.
    case eager
    /// Load when the image approaches the viewport.
    case lazy
}
