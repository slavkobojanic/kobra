// `match` arms bind payloads with their declared types, generic ones instantiated.
enum Fetch<T> {
    case idle
    case loaded(T)
    case failed(message: String)
}

func describe(state: Fetch<Int>) -> String {
    return match state {
        case .idle { "idle" }
        case .loaded(let value) { "{value + 1}" }
                       //^^^^^ type: Int
        case .failed(let message) { message }
                       //^^^^^^^ type: String
    }
}
