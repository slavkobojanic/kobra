// A struct bound with `let` can't be mutated through any path, even a `var` field.
struct Point {
    var x: Int
}

func move() -> Int {
    let p = Point(x: 1)
    p.x = 2
    return p.x
}
