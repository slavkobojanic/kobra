// A bare identifier as a match pattern in a view is K0115; the parser builds an IdentPattern.
component T {
    view {
        match state {
            case loaded {
                Done()
            }
        }
    }
}
