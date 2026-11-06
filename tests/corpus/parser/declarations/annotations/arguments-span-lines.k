// Newlines inside the argument list are insignificant.
component A {
    @Effect(
        on: [
            userId,
            filter,
        ],
    )
    func reload() {}
}
