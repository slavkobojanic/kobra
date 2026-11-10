/// The fixture's page: it counts in the worker from an effect, so asking for
/// another count, or stopping, cancels the one running.
page Home at "/" {
    @Inject let primes: Primes

    /// The count asked for; `0` when none is.
    @State var target: Int = 0

    /// What the page says about the last count.
    @State var status: String = "Idle"

    /// Counts in the worker whenever `target` changes. A change while it
    /// runs cancels the run, which terminates the worker.
    @Effect(on: [target])
    func count() async {
        guard target > 0 else {
            return
        }
        status = "Counting…"
        do {
            let total = try await primes.count(below: target)
            status = "{total} primes below {target}"
        } catch .tooLarge(let limit) {
            status = "At most {limit}"
        } catch .workerFailure(.crashed) {
            status = "The worker crashed."
        } catch .workerFailure(.unavailable) {
            status = "No worker."
        }
    }

    func countFew() {
        target = 100
    }

    func countMany() {
        target = 30000000
    }

    func countTooMany() {
        target = 1000000000
    }

    func stop() {
        target = 0
        status = "Stopped"
    }

    /// Crashes the worker on purpose.
    func crash() async {
        do {
            let value = try await primes.crash(at: 3)
            status = "Read {value}"
        } catch .workerFailure(.crashed) {
            status = "The worker crashed."
        } catch .workerFailure(.unavailable) {
            status = "No worker."
        }
    }

    view {
        heading(level: 1) { "Workers fixture" }

        button { "Count below 100" }
            .on(click: countFew)

        button { "Count below 30,000,000" }
            .on(click: countMany)

        button { "Count too many" }
            .on(click: countTooMany)

        button { "Stop" }
            .on(click: stop)

        button { "Crash" }
            .on(click: crash)

        paragraph { status }
            .role(.status)
    }
}
