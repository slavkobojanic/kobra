// Pattern fields can be written with their labels, mixing bindings, literals and wildcards.
enum Box {
    case sized(width: Int, height: Int)
}

func describe(box: Box) -> String {
    return match box {
        case .sized(width: 0, height: _) { "flat" }
        case .sized(width: let w, height: 0) { "line {w}" }
        case .sized(width: let w, height: let h) { "{w}x{h}" }
    }
}

func main() -> List<String> {
    return [describe(box: .sized(width: 0, height: 5)), describe(box: .sized(width: 3, height: 0)), describe(box: .sized(width: 2, height: 2))]
}
