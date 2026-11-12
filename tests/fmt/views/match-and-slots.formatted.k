component Status {
    @Prop let state: State

    view {
        match state {
            case .loading {
                Spinner()

                "Loading"
            }
            default { "Done" }
        }

        List(items: state.items) { "Header" }.row { item =>
            text { item.name }

            text { item.price }
        }
    }

    loading {
        Spinner()

        "Loading"
    }

    failed(let error) {
        "Failed"

        button { "Retry" }
            .on(click: retry)
            .class(.retry)
    }
}
