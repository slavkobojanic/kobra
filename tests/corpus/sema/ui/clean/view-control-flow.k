// `if`/`else`, `if let`, keyed `for` and `match` in a view (05-views.md).
enum Fetch {
    case idle
    case loading
    case loaded(String)
    case failed(String)
}

component Profile {
    @State var state: Fetch = .idle
    @State var nickname: String? = nil
    @State var tags: List<String> = []

    view {
        if tags.isEmpty {
            paragraph { "No tags" }
        } else {
            for tag in tags key tag {
                text { tag }
            }
        }

        if let nickname {
            text { nickname }
        }

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
