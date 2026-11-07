/// A color: `#3B82F6`, a theme token such as `.primary`, or a computed
/// color. Computed at compile time where possible, otherwise `color-mix()`
/// or a custom property (`docs/design/07-themes-and-units.md`, "Colors").
@Native
public struct Color implements Equatable, Codable {
    /// Returns the color of hex text such as `"#3B82F6"`.
    static func hex(_ text: String) -> Color
    /// Returns the color of red, green and blue components, each `0...255`.
    static func rgb(_ red: Int, _ green: Int, _ blue: Int) -> Color
    /// Opaque white, `#FFFFFF`.
    static let white: Color
    /// Opaque black, `#000000`.
    static let black: Color
    /// A fully transparent color.
    static let transparent: Color

    /// Returns the color with its alpha multiplied by `alpha`
    /// (`0.0...1.0`).
    func opacity(_ alpha: Double) -> Color
    /// Returns the color mixed with `other`.
    ///
    /// - Parameters:
    ///   - other: the color mixed in.
    ///   - amount: the share of this color in the result, the rest being
    ///     `other` (CSS `color-mix(in srgb, self amount, other)`).
    func mix(_ other: Color, _ amount: Percent) -> Color
}
