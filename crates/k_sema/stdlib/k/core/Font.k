/// The `font` token category: a family and a size, used by the `font`
/// property. Theme font tokens produce preload tags automatically.
public struct Font implements Equatable, Codable {
    /// The font family, such as `"Inter"`.
    let family: String
    /// The font size.
    let size: Length
}
