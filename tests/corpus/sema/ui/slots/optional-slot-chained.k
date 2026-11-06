// An optional slot (`View? = nil`) is chained with a block and unwrapped with
// `if let` inside; the unwrapped name is a `View`.
component Card {
    @Slot let content: View
    @Slot let footer: View? = nil

    view {
        article {
            content

            if let footer {
                footer
              //^^^^^^ type: View
            }
        }
    }
}

component Home {
    view {
        Card {
            "Body"
        }
        .footer {
            button { "Checkout" }
        }
        //@ 24:10-24:16 resolves: member corpus.Card.footer

        Card {
            "No footer"
        }
    }
}
