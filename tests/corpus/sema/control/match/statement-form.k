// `match` used as a statement must be exhaustive too.
enum Light {
    case red
    case amber
    case green
}

func log(light: Light) {
    match light {
        case .red {
            stop()
        }
        case .green {
            go()
        }
    }
}

func stop() {}

func go() {}
