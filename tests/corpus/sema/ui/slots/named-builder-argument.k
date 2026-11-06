// Several required slots: the unnamed one is the trailing block and the other is
// a named builder argument (04-components.md: "Other slots follow the
// named-or-chained rule").
component Split {
    @Slot let side: View
    @Slot let content: View

    view {
        box {
            side

            content
        }
    }
}

component Home {
    view {
        Split(side: { text { "Menu" } }) {
            paragraph { "Main" }
        }
    }
}
