// Links on an enum resolve against its cases; on a case, a qualified link to a sibling resolves.

/// A traffic light; [`red`] means stop.
enum Light {
    /// Stop.
    case red
    /// Follows [`Light.red`], never [`Light.blue`].
    case green
}
