// `Result<T, E>` has two cases; both handled is exhaustive, one alone isn't.
enum ParseError implements Error {
    case empty
}

func value(result: Result<Int, ParseError>) -> Int {
    return match result {
        case .success(let n) { n }
        case .failure { 0 }
    }
}

func valueOnly(result: Result<Int, ParseError>) -> Int {
    return match result {
        case .success(let n) { n }
    }
}
