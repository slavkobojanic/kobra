/// A system preference a theme variant can follow:
/// `variant dark matches .prefersDark { … }`.
public enum MediaCondition {
    /// `prefers-color-scheme: dark`.
    case prefersDark
    /// `prefers-contrast: more`.
    case prefersContrast
    /// `prefers-reduced-motion: reduce`.
    case prefersReducedMotion
}
