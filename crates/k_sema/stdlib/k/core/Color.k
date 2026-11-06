/// A color: `#3B82F6`, a theme token such as `.primary`, or a computed
/// color. Computed at compile time where possible, otherwise `color-mix()`
/// or a custom property (`docs/design/07-themes-and-units.md`, "Colors").
@Native
public struct Color implements Equatable, Codable {
    /// A color from hex text such as `"#3B82F6"`.
    static func hex(_ text: String) -> Color
    /// A color from red, green and blue components, each `0...255`.
    static func rgb(_ red: Int, _ green: Int, _ blue: Int) -> Color
    /// White.
    static let white: Color
    /// Black.
    static let black: Color
    /// A fully transparent color.
    static let transparent: Color

    /// The color with its alpha multiplied by `alpha` (`0.0...1.0`).
    func opacity(_ alpha: Double) -> Color
    /// The color mixed with `other` by `amount`.
    func mix(_ other: Color, _ amount: Percent) -> Color
}
