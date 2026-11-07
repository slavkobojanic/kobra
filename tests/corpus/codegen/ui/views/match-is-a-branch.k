// `match` on an enum is a `branch` block selecting an arm by tag; several patterns in one arm
// share it, and payload bindings read the payload (codegen.md §5, §11.1).
enum Fetch {
    case idle
    case loading
    case loaded(String)
    case failed(message: String)
}

component Profile {
    @State var state: Fetch = .idle

    view {
        box {
            match state {
                case .idle, .loading {
                    text { "…" }
                }

                case .loaded(let name) {
                    heading(level: 2) { name }
                }

                case .failed(let message) {
                    paragraph { message }
                        .role(.alert)
                }
            }
        }
    }
}
