// A list of interface values accepts every conforming type.
interface Shape {
    func area() -> Double
}

struct Square implements Shape {
    let side: Double

    func area() -> Double {
        return side * side
    }
}

struct Circle implements Shape {
    let radius: Double

    func area() -> Double {
        return radius * radius * 3.14
    }
}

let shapes: List<Shape> = [Square(side: 1.0), Circle(radius: 1.0)]
let areas = shapes.map { shape => shape.area() }
  //^^^^^ type: List<Double>
