// A qualified case pattern names the enum explicitly.
enum Light {
    case red
    case amber
    case green
}

func isStop(light: Light) -> Bool {
    return match light {
        case Light.red { true }
        //         ^^^ resolves: member corpus.Light.red
        default { false }
    }
}
