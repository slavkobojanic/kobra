test Choice {
    test "the named case passes" {
        expect throws Choice.second {
            try failWith(.second)
        }
    }

    test "another case of the same error fails" {
        expect throws Choice.second {
            try failWith(.first)
        }
    }
}
