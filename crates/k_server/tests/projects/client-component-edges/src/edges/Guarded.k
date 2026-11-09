import k.ui.ErrorBoundary

/// A panic in a client-rendered `view` reaches the `ErrorBoundary` around it.
public page Guarded at "/guarded" {
    @State var after: String = ""

    view {
        ErrorBoundary {
            Broken()
        }
        .fallback {
            paragraph { "Something broke" }
        }

        textField(value: bind after)
            .label("After")

        paragraph { "Typed: {after}" }
    }
}
