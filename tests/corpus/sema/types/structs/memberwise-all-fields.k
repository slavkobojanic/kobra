// Every stored field is a named argument of the memberwise initializer, in declaration order.
struct Point {
    let x: Int
    var y: Int
}

let origin = Point(x: 0, y: 0)
  //^^^^^^ type: corpus.Point
           //^^^^^ resolves: decl corpus.Point
