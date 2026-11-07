// Extensions add methods and statics to the program's own structs and enums.
struct Point implements Codable {
    let x: Int
    let y: Int
}

enum Axis {
    case horizontal
    case vertical
}

extend Point {
    static func zero() -> Point {
        return Point(x: 0, y: 0)
    }

    func flipped() -> Point {
        return Point(x: y, y: x)
    }
}

extend Axis {
    var other: Axis {
        match self {
            case .horizontal { .vertical }
            case .vertical { .horizontal }
        }
    }
}

func main() -> List<String> {
    let p = Point(x: 1, y: 2).flipped()
    let origin: Point = .zero()
    return ["{p.x},{p.y}", "{origin.x},{origin.y}", "{Axis.horizontal.other}"]
}
