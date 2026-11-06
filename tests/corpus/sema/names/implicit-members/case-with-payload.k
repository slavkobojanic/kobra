// A case with a payload is called like a function; the member is the case.
enum Shape {
    case circle(radius: Int)
    case square(side: Int)
}

let shape: Shape = .circle(radius: 3)
//                  ^^^^^^ resolves: member corpus.Shape.circle
