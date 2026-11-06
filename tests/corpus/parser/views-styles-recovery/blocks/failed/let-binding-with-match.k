// `failed(let error)` with an exhaustive match (from the async data design).
page Product {
    failed(let error) {
        match error {
            case .notFound { NotFound() }

            case .network(let e) { RetryPanel(error: e) }
        }
    }
}
