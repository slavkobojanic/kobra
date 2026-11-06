// path: src/corpus/Cart.test.k
// In a test, `use <expression> for C` is allowed only when `C` is a config.
service CartService {
    @State var items: List<Int> = []
}

test CartService {
    use 42 for CartService

    test "runs" {
        expect true
    }
}
