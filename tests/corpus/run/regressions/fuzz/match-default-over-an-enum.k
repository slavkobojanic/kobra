// Fuzz regression (stage 3 `differential`, typed programs): a `match` over an enum with a `default`
// arm fails to load, "More than one default clause in switch statement": the generated `switch`
// over the tag gets the arm's `default:` and the exhaustive match's own `default: $k.unreachable()`.
enum Light {
    case off
    case on(level: Int)
    case blinking
}

func describe(_ light: Light) -> String {
    return match light {
        case .off { "off" }
        default { "lit" }
    }
}

func main() -> List<String> {
    return [describe(.off), describe(.on(level: 2)), describe(.blinking)]
}
