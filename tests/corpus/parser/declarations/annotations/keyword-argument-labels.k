// Argument labels inside annotations may be keywords.
component A {
    @Effect(on: [a], in: .route, for: b) func f() {}
}
