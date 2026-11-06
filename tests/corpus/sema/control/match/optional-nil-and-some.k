// An optional has two constructors, `nil` and some value: `case nil` plus a
// binding catch-all, or `.some(let x)`, is exhaustive; the binding has the
// unwrapped type.
func describe(count: Int?) -> String {
    let text = match count {
        case nil { "none" }
        case .some(let value) { "{value}" }
//                     ^^^^^ type: Int
    }
    return text
}

func describeAll(count: Int?) -> String {
    return match count {
        case nil { "none" }
        case let value { "{value}" }
//               ^^^^^ type: Int?
    }
}
