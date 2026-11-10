page Home at "/" {
    @Inject let sieve: Sieve

    @State var shown: String = ""

    func run() async {
        do {
            let tally = try await sieve.count(below: 100)
            shown = label(tally.primes)
        } catch .workerFailure {
            shown = "none"
        }
    }

    view {
        button { "Count" }
            .on(click: run)

        paragraph { shown }
    }
}

func label(_ n: Int) -> String {
    return "{n} primes"
}
