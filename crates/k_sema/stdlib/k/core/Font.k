/// The `font` token category: a family and a size, used by the `font`
/// property. A font token is followed by a generic fallback stack, so text
/// keeps a readable face where the family isn't installed.
public struct Font implements Equatable, Codable {
    /// The font family, such as `"Inter"`.
    let family: String
    /// The font size, a [`Length`].
    let size: Length
}
