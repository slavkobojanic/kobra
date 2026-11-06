// A parameter of an unresolved type is K1202 at the type; the match over it is
// not checked further.
func label(light: Lamp) -> String {
    return match light {
        case .red { "stop" }
    }
}
