// Extensions may add static members, reachable as implicit members of the type.
struct Point {
    let x: Int
    let y: Int
}

extend Point {
    static func zero() -> Point {
        return Point(x: 0, y: 0)
    }
}

let origin: Point = .zero()
                   //^^^^ resolves: member corpus.Point.zero()
