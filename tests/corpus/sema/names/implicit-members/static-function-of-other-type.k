// Only static functions returning the expected type are candidates: another one is K2006, and no K2001 follows.
struct Point {
    let x: Int

    static func count() -> Int {
        return 1
    }
}

let point: Point = .count()
