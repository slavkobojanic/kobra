// Extending a type that doesn't exist is K1202 only; nothing inside reports a follow-on error.
extend Strng {
    var shout: String {
        uppercased()
    }
}
