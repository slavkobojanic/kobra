// A parameter needs a default and a type the panel has a control for.
struct Point {
    let x: Int
}

enum Shape {
    case circle(radius: Int)
    case square
}

component Dot {
    view {
        "dot"
    }
}

preview (count: Int, points: List<Int> = [], at: Point = Point(x: 1), shape: Shape = .square, maybe: Int? = nil) {
    Dot()
}
