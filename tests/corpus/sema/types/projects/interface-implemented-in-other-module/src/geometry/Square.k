// Conforms across modules and gets the default method.
import shapes.Shape

public struct Square implements Shape {
    let side: Double

    func area() -> Double {
        return side * side
    }
}
