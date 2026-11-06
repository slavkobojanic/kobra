// A type that exists with a member that doesn't is unresolved: K1901 over the whole dotted name.
enum Light {
    case red
    case green
}

/// Starts at [`Light.red`], never [`Light.purple`].
func start() -> Light {
    return .red
}
