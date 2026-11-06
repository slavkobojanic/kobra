// A default method may call the interface's other members.
interface Shape {
    func area() -> Double

    func isLarge() -> Bool {
        return area() > 100.0
             //^^^^ resolves: member corpus.Shape.area()
    }
}
