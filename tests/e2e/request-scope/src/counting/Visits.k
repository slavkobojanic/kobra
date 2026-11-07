/// An `.app` service: one per request on the server, one per app in the
/// browser.
@Scope(.app)
public service Visits {
    @State var count: Int = 0

    /// Counts one visit and returns the total so far.
    func bump() -> Int {
        count = count + 1

        return count
    }
}
