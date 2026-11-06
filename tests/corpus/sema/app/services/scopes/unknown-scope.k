// An unknown scope is an unknown implicit member of `ServiceScope`, and nothing else is reported.
@Scope(.forever)
service Cache {
    @State var size: Int = 0
}

service Reader {
    @Inject let cache: Cache
}
