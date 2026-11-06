// A `.request` service is server-only, so a view page can't inject it.
@Scope(.request)
service RequestLog {
    @State var lines: List<String> = []
}

page Log {
    @Inject let log: RequestLog

    view {
        text { "log" }
    }
}
