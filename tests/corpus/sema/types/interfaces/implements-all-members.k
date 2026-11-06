// A struct that provides every required member conforms; calls through the interface resolve to its requirement.
interface Shape {
    func area() -> Double
    var name: String
}

struct Square implements Shape {
    let side: Double
    let name: String

    func area() -> Double {
        return side * side
    }
}

func describe(shape: Shape) -> Double {
    return shape.area()
               //^^^^ resolves: member corpus.Shape.area()
}

let total = describe(shape: Square(side: 2.0, name: "sq"))
  //^^^^^ type: Double
