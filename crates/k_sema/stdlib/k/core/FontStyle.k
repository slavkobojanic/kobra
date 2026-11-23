/// Whether a [`FontFace`] draws upright or italic text, its
/// `@font-face` rule's `font-style`.
public enum FontStyle implements Equatable, Codable {
    /// Upright text.
    case normal
    /// Italic text.
    case italic
}
