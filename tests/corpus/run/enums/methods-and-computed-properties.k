// Enum methods and computed properties switch on `self`.
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

func main() -> List<Bool> {
    let d = Direction.up
    return [d.isUp(), d.flipped.isUp(), d.flipped.flipped.isUp()]
}
