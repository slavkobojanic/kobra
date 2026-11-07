// `match` arms with payload bindings and `where` guards; the first matching arm wins.
import k.observe.Log

enum Shape {
    case circle(radius: Int)
    case square(side: Int)
}

func describe(shape: Shape) -> String {
    return match shape {
        case .circle(let r) where r > 10 { "big circle" }
        case .circle { "circle" }
        case .square(let s) { "square {s}" }
    }
}

func main() -> List<String> {
    Log.info(describe(shape: .circle(radius: 20)))
    return [describe(shape: .circle(radius: 1)), describe(shape: .square(side: 2))]
}
