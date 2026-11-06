// An extension adds methods to a struct; they resolve as members of the type.
struct Point {
    let x: Int
    let y: Int
}

extend Point {
    func manhattan() -> Int {
        return x + y
    }
}

let d = Point(x: 1, y: 2).manhattan()
  //^ type: Int
                        //^^^^^^^^^ resolves: member corpus.Point.manhattan()
