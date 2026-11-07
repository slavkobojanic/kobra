// An `if` over `@State` writes runs only the branch taken.
test Shelves {
    @Inject let shelves: Shelves

    test "only the taken branch removes" {
        shelves.move(fromLeft: true)

        expect shelves.left == [2, 3]
        expect shelves.right == [1]
    }

    test "the other branch, from the other shelf" {
        shelves.move(fromLeft: true)
        shelves.move(fromLeft: false)

        expect shelves.left == [2, 3, 1]
        expect shelves.right.isEmpty
    }
}
