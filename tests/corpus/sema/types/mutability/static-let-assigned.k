// A `static let` is immutable.
struct Limits {
    static let max = 10
}

func raise() {
    Limits.max = 20
}
