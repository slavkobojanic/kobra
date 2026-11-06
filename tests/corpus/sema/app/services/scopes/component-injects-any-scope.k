// A `.component` service may inject services of every scope.
@Scope(.server)
service Counters {
    @State var hits: Int = 0
}

@Scope(.route)
service Filters {
    @State var query: String = ""
}

@Scope(.component)
service Field {
    @State var text: String = ""
}

@Scope(.component)
service FormState {
    @Inject let counters: Counters
    @Inject let filters: Filters
    @Inject let field: Field
}
