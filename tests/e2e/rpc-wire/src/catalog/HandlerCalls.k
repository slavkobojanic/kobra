/// Counts calls that reached a handler, process-wide, so a test can see
/// that validation refused a request before the handler ran.
@Scope(.server)
public service HandlerCalls {
    @State var count: Int = 0

    /// Records one call.
    func record() {
        count = count + 1
    }
}
