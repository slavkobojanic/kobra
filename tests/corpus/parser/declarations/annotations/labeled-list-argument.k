// `@Effect(on: [x])` takes a labeled list argument.
component Profile {
    @Effect(on: [userId])
    func reload() {}
}
