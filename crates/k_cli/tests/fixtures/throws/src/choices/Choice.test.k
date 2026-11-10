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

    test "the same case position of another error fails" {
        expect throws Choice.second {
            try failOther(.second)
            try failWith(.second)
        }
    }

    test "the named case passes beside another error" {
        expect throws Choice.second {
            try failWith(.second)
            try failOther(.first)
        }
    }
}
