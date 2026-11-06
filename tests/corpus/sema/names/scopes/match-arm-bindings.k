// A pattern binding is visible in its own arm, including the arm's `where` guard.
enum Shape {
    case circle(Int)
    case square(Int)
}

func size(shape: Shape) -> Int {
    return match shape {
        case .circle(let radius) where radius > 10 {
        //                             ^^^^^^ resolves: local radius 9:26
            radius
          //^^^^^^ resolves: local radius 9:26
        }
        case .circle(let radius) {
            radius * 2
          //^^^^^^ resolves: local radius 14:26
        }
        case .square(let side) {
            side
          //^^^^ resolves: local side 18:26
        }
    }
}
