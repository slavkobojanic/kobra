// path: src/corpus/Groups.test.k
// Test group names are free identifiers: they need not resolve, may repeat, and a file may hold
// several groups.
test Checkout {
    test "first" {
        expect 1 + 1 == 2
    }
}

test Checkout {
    test "second" {
        expect 2 * 2 == 4
    }
}

test NoSuchDeclaration {
    test "third" {
        expect true
    }
}
