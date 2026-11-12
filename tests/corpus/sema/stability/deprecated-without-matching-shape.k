// When the replacement's labels differ, or there is no replacement, the
// K1401 warning has no fix: the call would need other arguments.

@Deprecated("use `total(in:)`", since: "0.1", replacement: "total(in:)")
func sum(of values: List<Int>) -> Int {
    return 0
}

func total(in values: List<Int>) -> Int {
    return 0
}

@Deprecated("points are kept as `Vector`s now", since: "0.1")
struct Point {
    let x: Int
}

func report(p: Point) -> Int {
    return sum(of: [p.x])
}
