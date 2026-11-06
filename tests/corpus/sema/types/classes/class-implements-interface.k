// A class can implement an interface and be used where the interface is expected.
interface Clearable {
    func clear()
}

class Store implements Clearable {
    var items: List<String> = []

    func clear() {
        items.removeAll()
    }
}

func reset(target: Clearable) {
    target.clear()
         //^^^^^ resolves: member corpus.Clearable.clear()
}

func run() {
    reset(target: Store())
}
