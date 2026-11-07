// `case let other` binds the whole value for every case not handled earlier.
enum Token {
    case number(Int)
    case plus
    case minus
}

func show(token: Token) -> String {
    return match token {
        case .number(let n) { "{n}" }
        case let other { "op {other}" }
    }
}

func main() -> List<String> {
    return [show(token: .number(4)), show(token: .plus), show(token: .minus)]
}
