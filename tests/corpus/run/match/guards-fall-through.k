// A failing `where` guard moves on to the next arm; guards may use outer variables.
enum Reading {
    case value(Int)
    case missing
}

func classify(reading: Reading, limit: Int) -> String {
    return match reading {
        case .value(let v) where v > limit { "high {v}" }
        case .value(let v) where v < 0 { "invalid {v}" }
        case .value(let v) { "ok {v}" }
        case .missing { "missing" }
    }
}

func main() -> List<String> {
    return [classify(reading: .value(11), limit: 10), classify(reading: .value(10), limit: 10), classify(reading: .value(-1), limit: 10), classify(reading: .missing, limit: 0)]
}
