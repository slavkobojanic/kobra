// Case patterns resolve against the scrutinee; arm values against the match's expected type.
enum Light {
    case red
    case amber
    case green
}

func next(light: Light) -> Light {
    return match light {
        case .red { .green }
        //    ^^^ resolves: member corpus.Light.red
        //           ^^^^^ resolves: member corpus.Light.green
        case .green { .amber }
        case .amber { .red }
    }
}
