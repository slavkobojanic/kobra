// `@Emits`, `@Slot` and `@Stylable` all require `let`; one K3001 each.
component Panel {
    @Emits var closed: Event<Void>
    @Slot var content: View
    @Stylable var container: Classes

    view {
        box {
            content
        }
        .class(container)
    }
}
