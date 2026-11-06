// Every case handled: exhaustive, no `default` needed. `match` is an
// expression whose type is the arms' common type.
enum Light {
    case red
    case amber
    case green
}

func label(light: Light) -> String {
    let text = match light {
//      ^^^^ type: String
        case .red { "stop" }
//            ^^^ resolves: member corpus.Light.red
        case .amber { "wait" }
        case .green { "go" }
    }
    return text
}
