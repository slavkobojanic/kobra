// Against `T?`, `.name` resolves against `T`.
enum Light {
    case red
    case amber
    case green
}

let maybe: Light? = .amber
//                   ^^^^^ resolves: member corpus.Light.amber
