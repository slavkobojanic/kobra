// A binding of one arm is not visible in the next arm.
enum Shape {
    case circle(Int)
    case square(Int)
}

func size(shape: Shape) -> Int {
    return match shape {
        case .circle(let radius) { radius }
        case .square(let side) { radius }
    }
}
