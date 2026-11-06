// A fake conforms to every non-private member of the service: `add(item:)` is missing.
service FakeCartService implements CartService {
    @State var items: List<Int> = []
}
