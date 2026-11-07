// A `guard`'s `else` may throw.
enum Invalid implements Error {
    case empty
}

func first(of values: List<String>) throws Invalid -> String {
    guard let value = values.first else {
        throw Invalid.empty
    }
    return value
}

func main() -> List<String> {
    let none: List<String> = []
    return [try first(of: ["a", "b"]) else { "-" }, try first(of: none) else { "-" }]
}
