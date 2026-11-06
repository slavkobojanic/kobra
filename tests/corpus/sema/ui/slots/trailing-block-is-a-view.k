// The trailing block of a component call is view content: a `for` inside it
// needs a `key` like any view `for` (K3106).
component Panel {
    @Slot let content: View

    view {
        box {
            content
        }
    }
}

component Home {
    @State var names: List<String> = []

    view {
        Panel {
            for name in names {
                text { name }
            }
        }
    }
}
