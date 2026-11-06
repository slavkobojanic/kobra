// A `return` takes its expected type from the function's return type.
enum Light {
    case red
    case amber
    case green
}

func stop() -> Light {
    return .red
    //      ^^^ resolves: member corpus.Light.red
}
