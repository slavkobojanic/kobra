// A payload-free case is a frozen singleton, a payload case an object with its labelled fields, and
// `match` on an enum is a `switch` over the tag (docs/compiler/codegen.md §5, §6.2).
enum Light {
    case off
    case on(level: Int)
}

func brightness(light: Light) -> Int {
    return match light {
        case .off { 0 }
        case .on(let level) { level }
    }
}
