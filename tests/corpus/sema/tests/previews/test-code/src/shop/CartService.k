// The service a component injects.
public service CartService {
    @State var items: List<Int> = []

    func add(item: Int) {
        items.append(item)
    }
}
