import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

struct Tally implements Codable {
    let primes: Int
}

@Worker
public service Sieve {
    private let limit: Int = 1000

    func count(below: Int) async throws WorkerOnlyError -> Tally {
        var total = 0
        for n in 2..<below {
            if isPrime(n) {
                total += 1
            }
        }
        return Tally(primes: total)
    }

    func echo(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        return bytes
    }
}

func isPrime(_ n: Int) -> Bool {
    var d = 2
    while d * d <= n {
        if n % d == 0 {
            return false
        }
        d += 1
    }
    return true
}
