// An `.app` service can't inject a `.component` service.
@Scope(.component)
service FormState {
    @State var dirty: Bool = false
}

service Drafts {
    @Inject let form: FormState
}
