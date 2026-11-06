import k.auth.Session

@Scope(.server)
service Cache {
    @Inject let session: Session
}
