import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

/// Counts primes by trial division in a Web Worker: slow on purpose, so a
/// large count keeps the worker busy for seconds while the page stays
/// responsive.
@Worker
public service Primes {
    /// The largest count it accepts.
    private let limit: Int = 100000000

    /// The number of primes below `below`.
    func count(below: Int) async throws PrimeError -> Int {
        guard below <= limit else {
            throw .tooLarge(limit: limit)
        }
        var total = 0
        for n in 2..<below {
            if isPrime(n) {
                total += 1
            }
        }
        return total
    }

    /// The UTF-8 text of `bytes`, its characters in reverse order.
    func reversed(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        var characters: List<String> = []
        for c in (bytes.utf8Text ?? "").split(separator: "") {
            characters.insert(c, at: 0)
        }
        return characters.joined(separator: "").utf8
    }

    /// Panics: reads past the end of an empty list.
    func crash(at index: Int) async throws WorkerOnlyError -> Int {
        let none: List<Int> = []
        return none[index]
    }

    /// Whether `n` has no divisor but 1 and itself.
    private func isPrime(_ n: Int) -> Bool {
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
