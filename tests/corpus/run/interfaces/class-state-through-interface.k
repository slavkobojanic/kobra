// A class used through an interface keeps its identity: calls mutate the one instance.
interface Clearable {
    func clear()
}

class Store implements Clearable {
    var items: List<String> = ["a", "b"]

    func clear() {
        items.removeAll()
    }
}

func reset(target: Clearable) {
    target.clear()
}

func main() -> Int {
    let store = Store()
    reset(target: store)
    return store.items.count
}
