// A `match` over an enum in a view, with multiple patterns per arm and bindings (from the views design).
component T {
    view {
        match user {
            case .idle, .loading {
                Spinner()
            }

            case .loaded(let u) {
                ProfileCard(user: u)
            }

            case .failed(let error) {
                ErrorBanner(message: error.message)
                    .retry(load)
            }
        }
    }
}
