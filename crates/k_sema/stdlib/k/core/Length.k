/// A CSS length: `16px`, `1.5rem`, `50vh`, `1fr`
/// (`docs/design/07-themes-and-units.md`).
///
/// Lengths with the same unit fold at compile time (`16px * 2` is `32px`);
/// mixed units become `calc()`. `Spacing`, `Radius` and `FontSize` are its
/// token categories.
@Native
public struct Length implements Equatable, Codable {}
