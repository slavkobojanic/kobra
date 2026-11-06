// `init` declares initializers, with optional `throws` and visibility.
struct Product {
    let id: Int
    let name: String

    init(name: String) {
        self.id = 0
        self.name = name
    }

    private init() throws ValidationError {
        throw ValidationError.empty
    }
}
