// The service under test.
public service CartService {
    @State var items: List<Int> = []

    func add(item: Int) {
        items.append(item)
    }
}
