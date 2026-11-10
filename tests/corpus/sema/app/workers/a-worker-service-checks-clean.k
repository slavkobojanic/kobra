// docs/design/55-workers.md: a `@Worker` service with private constants and
// helpers, a method throwing its own Codable error enum and one throwing
// `WorkerOnlyError`, injected by a component that calls it from a handler
// and an effect. No diagnostics.
import k.concurrency.Worker
import k.concurrency.WorkerFailure
import k.concurrency.WorkerOnlyError

enum SieveError implements Error, Codable {
    case tooLarge(limit: Int)
    case workerFailure(WorkerFailure)
}

struct Tally implements Codable {
    let primes: Int
    let largest: Int?
}

@Worker
service Sieve {
    private let limit: Int = 1000000

    private var half: Int {
        limit / 2
    }

    func count(below: Int) async throws SieveError -> Tally {
        guard below <= limit else {
            throw .tooLarge(limit: limit)
        }
        return Tally(primes: primes(below).count, largest: primes(below).last)
    }

    func echo(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        return bytes
    }

    private func primes(_ n: Int) -> List<Int> {
        var found: List<Int> = []
        for i in 2..<n {
            if isPrime(i) {
                found.append(i)
            }
        }
        return found
    }

    private static func isPrime(_ n: Int) -> Bool {
        var d = 2
        while d * d <= n {
            if n % d == 0 {
                return false
            }
            d += 1
        }
        return true
    }
}

component Counter {
    @Inject let sieve: Sieve

    @State var below: Int = 100
    @State var shown: String = ""

    @Effect(on: [below])
    func recount() async {
        do {
            let tally = try await sieve.count(below: below)
            shown = "{tally.primes}"
        } catch .tooLarge(let limit) {
            shown = "at most {limit}"
        } catch .workerFailure {
            shown = "unavailable"
        }
    }

    func more() {
        below += 100
    }

    view {
        button { "More" }
            .on(click: more)

        paragraph { shown }
    }
}
