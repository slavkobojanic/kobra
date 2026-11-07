/// Counts up to a limit.
public service Counter {
    @State var value: Int = 0

    /// The largest value.
    let limit: Int = 3

    /// Adds one, up to the limit.
    func increment() throws CounterError {
        guard value < limit else {
            throw CounterError.atLimit
        }
        value = value + 1
    }

    /// Starts again from zero.
    func reset() {
        value = 0
    }
}
