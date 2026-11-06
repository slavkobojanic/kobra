// A required slot without a trailing block is a missing argument (K2003 before
// `)`).
component Card {
    @Prop let title: String
    @Slot let content: View

    view {
        article {
            content
        }
    }
}

component Home {
    view {
        Card(title: "Cart")
        Card(title: "Cart") {
            "ok"
        }
    }
}
