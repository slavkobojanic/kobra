// `.name` resolves against the expected type: here the parameter's enum type.
enum Light {
    case red
    case amber
    case green
}

func paint(light: Light) -> Int {
    return 0
}

let stop = paint(light: .red)
//                       ^^^ resolves: member corpus.Light.red
