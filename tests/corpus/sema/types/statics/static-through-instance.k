// A static member is not reachable through a value: K1203.
struct Point {
    let x: Int

    static let origin = Point(x: 0)
}

let p = Point(x: 1)
let o = p.origin
