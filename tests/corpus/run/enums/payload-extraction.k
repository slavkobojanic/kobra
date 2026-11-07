// Labelled and unlabelled payloads bind positionally in patterns.
enum Shape {
    case circle(radius: Int)
    case rect(width: Int, height: Int)
    case labelled(String, Int)
}

func describe(shape: Shape) -> String {
    return match shape {
        case .circle(let r) { "circle {r}" }
        case .rect(let w, let h) { "rect {w}x{h}" }
        case .labelled(let name, let size) { "{name}:{size}" }
    }
}

func main() -> List<String> {
    return [describe(shape: .circle(radius: 2)), describe(shape: .rect(width: 3, height: 4)), describe(shape: .labelled("tag", 7))]
}
