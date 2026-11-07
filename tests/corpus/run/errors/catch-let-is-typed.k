// `catch let` binds the error with its static type, so it can be matched again and interpolated.
enum Kind implements Error {
    case first
    case second(Int)
}

func fail(n: Int) throws Kind {
    if n == 0 {
        throw Kind.first
    }
    throw Kind.second(n)
}

func run(n: Int) -> String {
    do {
        try fail(n: n)
        return "none"
    } catch let error {
        return match error {
            case .first { "first" }
            case .second(let v) { "second {v}" }
        }
    }
}

func main() -> List<String> {
    return [run(n: 0), run(n: 5)]
}
