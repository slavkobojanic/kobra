// A public interface with one requirement and one default method.
public interface Shape {
    func area() -> Double

    func isLarge() -> Bool {
        return area() > 100.0
    }
}
