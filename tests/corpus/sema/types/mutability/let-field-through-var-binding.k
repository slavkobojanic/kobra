// A `let` field can't be assigned even through a `var` binding; the fix would be in the struct, so none is offered.
struct Point {
    let x: Int
    var y: Int
}

func move() -> Int {
    var p = Point(x: 1, y: 1)
    p.y = 2
    p.x = 2
    return p.x
}
