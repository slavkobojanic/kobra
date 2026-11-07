/// The theme the book's code samples are checked against.
///
/// Book chapters use tokens such as `.md`, `.muted` and `.lg` without
/// showing the app's theme each time. `crates/k_sema/tests/book.rs` adds
/// this file to every checked sample whose project has no theme of its own,
/// so a sample that uses a token the book never declares still fails.
public theme BookTheme {
    color primary: Color = #3B82F6
    color surface: Color = #FFFFFF
    color surfaceHover: Color = #F3F4F6
    color surfaceSunken: Color = #F5F5F4
    color text: Color = #111827
    color muted: Color = #6B7280
    color border: Color = #E5E7EB
    color danger: Color = #B91C1C
    color success: Color = #15803D

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px
    space xl: Spacing = 32px

    radius sm: Radius = 4px
    radius md: Radius = 8px
    radius lg: Radius = 16px

    fontSize sm: FontSize = 14px
    fontSize md: FontSize = 16px
    fontSize lg: FontSize = 20px
    fontSize xl: FontSize = 28px

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark {
        surface = #111827
        surfaceHover = #1F2937
        text = #F9FAFB
    }
}
