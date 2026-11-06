// Components, pages and layouts aren't scoped services: they may inject client-safe services of any
// scope.
@Scope(.route)
service Filters {
    @State var query: String = ""
}

@Scope(.component)
service FormState {
    @State var dirty: Bool = false
}

component Search {
    @Inject let filters: Filters
    @Inject let form: FormState

    view {
        text { filters.query }
    }
}
