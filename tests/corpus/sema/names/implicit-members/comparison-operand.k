// In `==`, the other operand's type is the expected type.
enum Light implements Equatable {
    case red
    case amber
    case green
}

func isStop(light: Light) -> Bool {
    return light == .red
    //               ^^^ resolves: member corpus.Light.red
}
