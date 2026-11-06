// A static func that doesn't return the expected type is not an implicit member of it: K2006.
struct Point {
    let x: Int

    static func count() -> Int {
        return 0
    }
}

let p: Point = .count()
