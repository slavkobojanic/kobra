// The same members are reachable explicitly through the type, with no expected type needed.
enum Light {
    case red
    case green
}

let paper = Color.white
//                ^^^^^ resolves: member Color.white
let stop = Light.red
//               ^^^ resolves: member corpus.Light.red
