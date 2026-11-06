// A test's `use X for Y` also requires X to be or implement Y.
service Unrelated {
    @State var count: Int = 0
}

test CartService {
    use Unrelated for CartService

    test "runs" {
        expect true
    }
}
