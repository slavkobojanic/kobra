// K2010 offers a maybe-incorrect fix inserting stubs for the missing members
// (diagnostics.md, types).
interface Shape {
    func area() -> Double
}

struct Square implements Shape {
    let side: Double
}
