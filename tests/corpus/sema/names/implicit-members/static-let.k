// Static `let`s of the expected type are implicit members, for stdlib and user types alike.
struct Point {
    let x: Int
    let y: Int

    static let origin = Point(x: 0, y: 0)
}

let start: Point = .origin
//                  ^^^^^^ resolves: member corpus.Point.origin
let paper: Color = .white
//                  ^^^^^ resolves: member Color.white
