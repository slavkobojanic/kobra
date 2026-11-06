// Enums can declare methods and computed properties; `self` is the enum value.
enum Direction {
    case up
    case down

    var flipped: Direction {
        match self {
            case .up { .down }
            case .down { .up }
        }
    }

    func isUp() -> Bool {
        return self == .up
    }
}

let d = Direction.up.flipped
  //^ type: corpus.Direction
let u = Direction.down.isUp()
  //^ type: Bool
