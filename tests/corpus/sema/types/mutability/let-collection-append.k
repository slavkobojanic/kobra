// A list bound with `let` is immutable: a mutating call is K2011 at the receiver.
func names() -> List<String> {
    let names = ["Ana"]
    names.append("Ben")
    return names
}
