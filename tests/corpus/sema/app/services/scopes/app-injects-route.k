// An `.app` service (the default) can't inject a `.route` service, which may end sooner.
@Scope(.route)
service Filters {
    @State var query: String = ""
}

service Search {
    @Inject let filters: Filters
}
