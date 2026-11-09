import k.test.TestPreferences

test Motion {
    @Inject let preferences: TestPreferences

    test "no preference is set by default" {
        let screen = render(Motion())

        expect screen.text("Carousel playing").isVisible
        expect not screen.text("Dark palette").exists
        expect not screen.text("High contrast").exists
    }

    test "each preference follows the system" {
        let screen = render(Motion())

        preferences.set(prefersReducedMotion: true)
        preferences.set(prefersDark: true)

        expect screen.text("Carousel paused").isVisible
        expect screen.text("Dark palette").isVisible

        preferences.set(forcedColors: true)

        expect screen.text("High contrast").isVisible

        preferences.set(forcedColors: false)
        preferences.set(prefersMoreContrast: true)

        expect preferences.prefersMoreContrast
        expect screen.text("High contrast").isVisible
    }
}
