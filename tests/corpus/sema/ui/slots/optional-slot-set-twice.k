// An optional slot follows the option rule: chaining it twice is K3104.
component Card {
    @Slot let content: View
    @Slot let footer: View? = nil

    view {
        box {
            content

            if let footer {
                footer
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
            "One"
        }
        .footer {
            "Two"
        }
    }
}
