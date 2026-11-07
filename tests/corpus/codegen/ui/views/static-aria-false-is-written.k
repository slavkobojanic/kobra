// A constant `false` ARIA state is still written as text: `aria-pressed="false"` means "a toggle
// that is off", which an absent attribute doesn't (WAI-ARIA).
component Toolbar {
    view {
        box {
            button { "Bold" }
                .pressed(true)

            button { "Italic" }
                .pressed(false)

            button { "Menu" }
                .expanded(false)
        }
    }
}
