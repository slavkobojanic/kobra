// Parameter labels are part of a requirement: a different label is K2015.
interface Store {
    func save(text: String)
}

struct Memory implements Store {
    func save(value: String) {}
}
