/// The `.loading(...)` attribute of `image`.
public enum ImageLoading {
    /// Loads the image at once.
    case eager
    /// Loads the image when it approaches the viewport.
    case lazy
}
