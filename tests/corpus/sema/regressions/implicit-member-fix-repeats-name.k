// A static member that isn't an implicit member of the expected type (another
// type, or a function not returning it) is K2006. The closest-name fix must not
// suggest the very name that was written: replacing `count` with `count` fixes
// nothing.
struct Point {
    let x: Int

    static let limit: Int = 9

    static func count() -> Int {
        return 0
    }
}

let a: Point = .count()
let b: Point = .limit
