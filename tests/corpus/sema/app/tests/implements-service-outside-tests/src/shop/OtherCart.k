// Outside test code, `implements` a service is an error: production code uses interfaces.
public service OtherCart implements CartService {
    @State var items: List<Int> = []

    func add(item: Int) {
        items.append(item)
    }
}
