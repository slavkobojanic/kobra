// A `.server` service can't inject a `.request` service.
@Scope(.request)
service RequestLog {
    @State var lines: List<String> = []
}

@Scope(.server)
service Audit {
    @Inject let log: RequestLog
}
