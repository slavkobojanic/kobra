// `static let` and `static func` are members of the type, not of its values.
struct Point {
    let x: Int
    let y: Int

    static let origin = Point(x: 0, y: 0)

    static func at(x: Int) -> Point {
        return Point(x: x, y: 0)
    }
}

let o = Point.origin
  //^ type: corpus.Point
let p = Point.at(x: 3)
            //^^ resolves: member corpus.Point.at(x:)
let q: Point = .origin
              //^^^^^^ resolves: member corpus.Point.origin
