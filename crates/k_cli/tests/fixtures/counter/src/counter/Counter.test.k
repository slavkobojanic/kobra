test Counter {
    @Inject let counter: Counter

    test "starts at zero" {
        expect counter.value == 0
    }

    test "increments by one" {
        try counter.increment()
        try counter.increment()

        expect counter.value == 2
    }

    test "every test gets a fresh counter" {
        expect counter.value == 0
    }

    test "stops at the limit" {
        try counter.increment()
        try counter.increment()
        try counter.increment()

        expect throws CounterError.atLimit {
            try counter.increment()
        }
    }

    test "a wrong expectation shows its values" {
        try counter.increment()

        expect counter.value + 1 == counter.limit
    }

    test "an error that isn't thrown fails" {
        expect throws CounterError.atLimit {
            try counter.increment()
        }
    }
}
