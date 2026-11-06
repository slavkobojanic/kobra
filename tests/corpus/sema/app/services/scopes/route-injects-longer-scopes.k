// A `.route` service may inject `.server`, `.app` and other `.route` services.
@Scope(.server)
service Counters {
    @State var hits: Int = 0
}

service Prefs {
    @State var dark: Bool = false
}

@Scope(.route)
service Filters {
    @State var query: String = ""
}

@Scope(.route)
service Results {
    @Inject let counters: Counters
    @Inject let prefs: Prefs
    @Inject let filters: Filters
}
