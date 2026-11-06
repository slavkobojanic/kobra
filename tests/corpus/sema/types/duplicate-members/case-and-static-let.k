// A case and a `static let` share the static namespace: `.x` would be
// ambiguous, so the later one is K2019.
enum E {
    case x
    static let x: E = .x
}
