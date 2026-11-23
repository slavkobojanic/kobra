// A fake in a test-only module, and a fixture.
import shop.CartService

public service FakeCart implements CartService {
    @State var items: List<Int> = []

    static let label = "fake"

    func add(item: Int) {
        items.append(item)
    }

    func seed(count: Int) {
        for i in 0..<count {
            items.append(i)
        }
    }
}

public extend Int {
    static let sample = 2
}
