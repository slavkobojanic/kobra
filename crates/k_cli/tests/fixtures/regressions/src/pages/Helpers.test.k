test Helpers {
    func describe(first: String = "a", second: String = "b") -> String {
        return "{first}{second}"
    }

    func describeLater(first: String = "a", second: String = "b") async throws Choice -> String {
        return "{first}{second}"
    }

    // A helper of a test group, called with a later labelled argument
    // only: the earlier parameter keeps its default (the argument went to
    // the first parameter).
    test "a labelled argument after a default goes to its own parameter" {
        expect describe(second: "z") == "az"
        expect describe(first: "y") == "yb"
        expect describe() == "ab"
    }

    func failUnless(first: String = "a", second: String = "b") async throws Choice {
        if first != "a" or second != "z" {
            throw Choice.first
        }
        throw Choice.second
    }

    test "the same holds for an async helper that throws" {
        expect try await describeLater(second: "z") == "az"
        expect try await describeLater() == "ab"
    }

    // Inside an `expect throws` block too (the call lost its signature, so
    // `second:` went to `first`).
    test "a labelled argument inside expect throws goes to its own parameter" {
        expect throws Choice.second {
            try await failUnless(second: "z")
        }
    }

    // `expect throws` checks the pattern: an error of another case fails it.
    test "expect throws matches the case, not any error" {
        var matched = 0
        do {
            try failWith(.second)
        } catch .first {
            matched = 1
        } catch .second {
            matched = 2
        }

        expect matched == 2
        expect throws Choice.second {
            try failWith(.second)
        }
    }
}

enum Choice implements Error {
    case first
    case second
}

func failWith(_ c: Choice) throws Choice {
    throw c
}
