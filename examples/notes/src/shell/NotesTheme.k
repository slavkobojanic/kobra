/// The app's design tokens: a quiet, mostly monochrome palette with one
/// accent for actions and one for pinned notes.
public theme NotesTheme {
    color primary: Color = #2563EB
    color foreground: Color = #0F172A
    color muted: Color = #64748B
    color surface: Color = #FFFFFF
    color surfaceRaised: Color = #F1F5F9
    color border: Color = #E2E8F0
    color highlight: Color = #F59E0B
    color danger: Color = #DC2626

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius sm: Radius = 4px
    radius md: Radius = 8px

    font body: Font = Font(family: "Inter", size: 16px)
}
