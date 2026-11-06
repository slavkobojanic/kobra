// A payload argument is checked against the case's field type.
enum Shape {
    case circle(radius: Double)
}

let c: Shape = .circle(radius: "big")
