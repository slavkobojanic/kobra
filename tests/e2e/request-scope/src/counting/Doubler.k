/// A second service that shares the request's `Visits`.
public service Doubler {
    @Inject let visits: Visits

    /// Bumps twice through the shared instance.
    func bumpTwice() async -> Int {
        visits.bump()

        return visits.bump()
    }
}
