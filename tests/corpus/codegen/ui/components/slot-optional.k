// An optional slot is `null` when not given; `if let footer` unwraps it like any optional
// (04-components.md, codegen.md §10).
component Card {
    @Slot let content: View
    @Slot let footer: View? = nil

    view {
        article {
            content

            if let footer {
                footer
            }
        }
    }
}

component Home {
    view {
        box {
            Card {
                paragraph { "plain" }
            }

            Card {
                paragraph { "with footer" }
            }
            .footer {
                button { "Checkout" }
            }
        }
    }
}
