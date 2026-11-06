// A `.request` service may inject `.server`, `.app` and `.request` services, but not `.route` ones.
@Scope(.route)
service Filters {
    @State var query: String = ""
}

@Scope(.request)
service RequestLog {
    @Inject let filters: Filters
}
