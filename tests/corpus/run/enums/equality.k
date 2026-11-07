// Enums with payloads compare structurally; payload-free cases are singletons.
enum Light implements Equatable {
    case off
    case on(level: Int)
}

func main() -> List<Bool> {
    let a: Light = .on(level: 3)
    let b: Light = .on(level: 3)
    let c: Light = .on(level: 4)
    let d: Light = .off
    return [a == b, a == c, d == .off, a != d]
}
