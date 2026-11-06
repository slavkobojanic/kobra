// Layouts may declare guards.
public layout Admin requires Authenticated, Role(.admin) {
    @Inject let session: Session
}
