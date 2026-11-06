interface Shape {
    func area() -> Double
}

struct Square implements Shape {
    let side: Double
}
