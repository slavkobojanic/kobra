// A group named like the service it tests, with a helper named like one of
// the service's functions: the helper's call keeps its own labels (it was
// lowered with the service's signature, so `second:` went to `first`).
test Tally {
    @Inject let tally: Tally

    func add(first: Int = 1, second: Int = 2) -> Int {
        return first * 10 + second
    }

    test "the helper, not the service's function of the same name" {
        expect add(second: 5) == 15
        expect tally.add(amount: 3) == 3
    }
}
