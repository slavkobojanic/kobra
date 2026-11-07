// A copy with no changes equals the original; a changed copy doesn't.
struct Settings implements Copyable, Equatable {
    let theme: String
    var size: Int
}

func main() -> List<Bool> {
    let base = Settings(theme: "dark", size: 2)
    return [base.copy() == base, base.copy(size: 3) == base, base.copy(size: 3).copy(size: 2) == base]
}
