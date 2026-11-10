/// `k test` runs the worker in-process, through the same codecs
/// (docs/design/55-workers.md, "Tests").
test Primes {
    @Inject let primes: Primes

    test "counts the primes below a number" {
        expect try await primes.count(below: 100) == 25
    }

    test "a typed error crosses with its payload" {
        expect throws PrimeError.tooLarge {
            try await primes.count(below: 1000000000)
        }
    }

    test "bytes cross both ways" {
        let reversed = try await primes.reversed(bytes: "abc".utf8)

        expect reversed.utf8Text == "cba"
    }

    test "a panic in the worker is a crash" {
        expect throws WorkerOnlyError.workerFailure {
            try await primes.crash(at: 3)
        }
    }

    test "the worker starts again after a crash" {
        expect throws WorkerOnlyError.workerFailure {
            try await primes.crash(at: 3)
        }

        expect try await primes.count(below: 10) == 4
    }
}
