// A static `let` of another type is not an implicit member of this one: K2006.
struct Point {
    let x: Int

    static let limit: Int = 9
}

let point: Point = .limit
