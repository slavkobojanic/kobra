// A `match` with no arms parses (exhaustiveness is semantic).
component T {
    view {
        match state {}
    }
}
