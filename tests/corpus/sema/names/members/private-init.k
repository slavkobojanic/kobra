// A private `init` can be called only inside its type, for example by a static factory; outside it is K1301 at the type name.
struct Token {
    let value: Int

    private init(value: Int) {
        self.value = value
    }

    static func make() -> Token {
        return Token(value: 1)
    }
}

let outside = Token(value: 2)
let inside = Token.make()
