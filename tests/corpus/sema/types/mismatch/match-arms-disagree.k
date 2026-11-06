// The arms of a `match` used as a value must agree.
enum Size {
    case small
    case large
}

func label(size: Size) -> String {
    return match size {
        case .small { "S" }
        case .large { 2 }
    }
}
