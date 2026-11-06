// Static functions returning the expected type are implicit members.
struct Point {
    let x: Int
    let y: Int

    static func make(x: Int, y: Int) -> Point {
        return Point(x: x, y: y)
    }
}

let corner: Point = .make(x: 3, y: 4)
//                   ^^^^ resolves: member corpus.Point.make(x:y:)
let brand: Color = .hex("#3B82F6")
//                  ^^^ resolves: member Color.hex(_:)
