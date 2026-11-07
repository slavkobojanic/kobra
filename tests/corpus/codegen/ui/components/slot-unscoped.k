// A required unnamed slot is the trailing block; it is a function returning nodes, placed between
// markers (04-components.md, codegen.md §10, §11.1).
component Card {
    @Slot let content: View

    view {
        article {
            content
        }
    }
}

component Home {
    view {
        Card {
            paragraph { "Review before checkout." }
        }
    }
}
