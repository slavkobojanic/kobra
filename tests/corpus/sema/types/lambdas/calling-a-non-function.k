// Calling a value that isn't a function is K2005 at the callee.
struct Point {
    let x: Int
}

func run(p: Point) -> Int {
    let n = 3
    let a = n()
    let b = p.x()
    return a + b
}
