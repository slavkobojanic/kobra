// The required unnamed `@Slot let content: View` is the trailing block. Placing
// the slot in the view resolves to the field.
component Card {
    @Prop let title: String
    @Slot let content: View

    view {
        article {
            heading(level: 2) { title }

            content
          //^^^^^^^ resolves: member corpus.Card.content
          //^^^^^^^ type: View
        }
    }
}

component Home {
    view {
        Card(title: "Cart") {
            paragraph { "Review before checkout." }
        }
    }
}
