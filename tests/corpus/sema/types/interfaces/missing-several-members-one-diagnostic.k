// Several missing members are one K2010 at the interface name in `implements`.
interface Store {
    func load() -> String
    func save(text: String)
    var isReady: Bool
}

struct Memory implements Store {
    func load() -> String {
        return ""
    }
}
