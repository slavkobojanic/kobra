// The element type of an expected `List<T>` is the expected type of each element.
enum Light {
    case red
    case amber
    case green
}

let cycle: List<Light> = [.red, .green, .amber]
//                               ^^^^^ resolves: member corpus.Light.green
