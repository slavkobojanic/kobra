// Type annotations on constants, fields and parameter defaults are expected types.
enum Light {
    case red
    case amber
    case green
}

let light: Light = .green
//                  ^^^^^ resolves: member corpus.Light.green

struct Signal {
    let light: Light = .amber
    //                  ^^^^^ resolves: member corpus.Light.amber
}

func paint(light: Light = .red) -> Int {
//                         ^^^ resolves: member corpus.Light.red
    return 0
}
