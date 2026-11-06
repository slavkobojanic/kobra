// A test-only module may fake a service too.
import shop.CartService

public service FakeCart implements CartService {
    @State var items: List<Int> = []

    func add(item: Int) {
    }
}
