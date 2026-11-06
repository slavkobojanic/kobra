// K2307 in a method is at the method body's closing brace.
struct Box {
    let width: Int

    func half() -> Int {
        if width > 0 {
            return width / 2
        }
    }
}
