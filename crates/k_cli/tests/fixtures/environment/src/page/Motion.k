import k.device.Preferences

/// A carousel that advances on its own only for users who don't prefer
/// reduced motion, and a chart palette that follows dark mode.
public component Motion {
    @Inject let preferences: Preferences

    view {
        if preferences.prefersReducedMotion {
            text { "Carousel paused" }
        } else {
            text { "Carousel playing" }
        }
        if preferences.prefersDark {
            text { "Dark palette" }
        }
        if preferences.prefersMoreContrast or preferences.forcedColors {
            text { "High contrast" }
        }
    }
}
