/// A `.server` service: one per server process, shared by every request.
@Scope(.server)
public service ProcessHits {
    @State var hits: Int = 0

    /// Counts one hit and returns the total.
    func hit() -> Int {
        hits = hits + 1

        return hits
    }
}
