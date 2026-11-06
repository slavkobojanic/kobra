// Classes are checked against their interfaces like structs.
interface Clearable {
    func clear()
}

class Store implements Clearable {
    var items: List<String> = []
}
