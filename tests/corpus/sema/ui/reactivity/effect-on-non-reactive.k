// The dependencies of `@Effect(on:)` must be reactive places (K2001): a literal
// and a plain field are not.
component Profile {
    @State var page: Int = 0
    var scratch: Int = 0

    @Effect(on: [page, 3, scratch])
    func reload() {
        scratch += 1
    }

    view {
        text { "{page}" }
    }
}
