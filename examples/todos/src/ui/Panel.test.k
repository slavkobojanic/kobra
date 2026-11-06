test Panel {
    test "by default, the panel is a level 2 heading and its content" {
        let screen = render(
            Panel(title: "Groceries") {
                paragraph { "Milk, eggs, bread." }
            }
        )

        expect screen.heading(named: "Groceries").level == 2
        expect screen.text("Milk, eggs, bread.").isVisible
        expect screen.buttons().isEmpty
    }

    test "the heading level can be set for a page's main panel" {
        let screen = render(
            Panel(title: "Groceries") {
                paragraph { "Milk, eggs, bread." }
            }
            .headingLevel(1)
        )

        expect screen.heading(named: "Groceries").level == 1
    }

    test "the toolbar and actions slots are placed when given" {
        let screen = render(
            Panel(title: "Groceries") {
                paragraph { "Milk, eggs, bread." }
            }
            .toolbar {
                button { "Settings" }
            }
            .actions {
                button { "Clear" }
            }
        )

        expect screen.button(named: "Settings").isVisible
        expect screen.button(named: "Clear").isVisible
    }
}
