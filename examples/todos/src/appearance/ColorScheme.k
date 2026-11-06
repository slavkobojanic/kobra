/// The two color schemes the app's theme defines.
public enum ColorScheme implements Codable, Equatable {
    /// Dark text on light surfaces: the theme's base tokens.
    case light

    /// Light text on dark surfaces: the theme's `dark` variant.
    case dark
}
