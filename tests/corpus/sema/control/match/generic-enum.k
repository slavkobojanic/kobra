// Generic enums are matched like any other: `LoadState<T>` from the prelude.
func describe(state: LoadState<Int>) -> String {
    return match state {
        case .idle, .loading { "…" }
        case .loaded(let value) { "{value}" }
//                       ^^^^^ type: Int
        case .failed(let error) { "{error}" }
    }
}
