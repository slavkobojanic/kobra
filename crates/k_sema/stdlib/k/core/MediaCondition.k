/// A system preference a theme variant can follow:
/// `variant dark matches .prefersDark { … }`.
public enum MediaCondition {
    /// The user prefers a dark color scheme (`prefers-color-scheme: dark`).
    case prefersDark
    /// The user prefers more contrast (`prefers-contrast: more`).
    case prefersContrast
    /// The user prefers reduced motion (`prefers-reduced-motion: reduce`).
    case prefersReducedMotion
}
