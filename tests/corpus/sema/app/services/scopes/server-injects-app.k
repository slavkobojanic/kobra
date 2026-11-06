// A `.server` service lives for the whole process and may inject only `.server` services.
service Prefs {
    @State var dark: Bool = false
}

@Scope(.server)
service Counters {
    @State var hits: Int = 0
}

@Scope(.server)
service Stats {
    @Inject let prefs: Prefs
    @Inject let counters: Counters
}
