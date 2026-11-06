// Instance members are never implicit members, even with a matching type: K2006.
struct Point {
    let x: Int

    var mirrored: Point {
        Point(x: 0 - x)
    }
}

let point: Point = .mirrored
