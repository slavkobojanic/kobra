import k.ui.Theme

test Home {
    @Inject let theme: Theme

    test "the dark mode switch changes the theme variant" {
        let screen = render(Home())

        expect theme.current == .base

        screen.checkbox(labeled: "Dark mode").click()

        expect theme.current == .dark

        screen.checkbox(labeled: "Dark mode").click()

        expect theme.current == .base
    }
}
