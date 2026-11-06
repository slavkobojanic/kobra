// A payload case is constructed like a call with its labels.
enum Shape {
    case circle(radius: Double)
    case rect(width: Double, height: Double)
}

let c: Shape = .circle(radius: 2.0)
             //^^^^^^^^^^^^^^^^^^^^ type: corpus.Shape
let r = Shape.rect(width: 1.0, height: 2.0)
  //^ type: corpus.Shape
