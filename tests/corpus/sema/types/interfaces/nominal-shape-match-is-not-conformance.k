// Interfaces are nominal: a type with the right members but no `implements` doesn't conform.
interface Shape {
    func area() -> Double
}

struct Circle {
    let radius: Double

    func area() -> Double {
        return radius * radius * 3.14
    }
}

func describe(shape: Shape) -> Double {
    return shape.area()
}

let a = describe(shape: Circle(radius: 1.0))
