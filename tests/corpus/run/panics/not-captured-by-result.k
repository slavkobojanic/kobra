// `Result.of` captures typed errors, not panics.
enum Never implements Error {
    case never
}

func pick(values: List<Int>) throws Never -> Int {
    if values.isEmpty {
        throw Never.never
    }
    return values[5]
}

func main() -> Int {
    let result: Result<Int, Never> = Result.of { try pick(values: [1]) }
    return match result {
        case .success(let v) { v }
        case .failure { -1 }
    }
}
