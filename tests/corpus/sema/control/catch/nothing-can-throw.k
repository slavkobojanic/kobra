// When nothing in the `do` block can throw, no `catch` clause can run: K2302
// at the `catch` keyword.
func one() -> Int {
    return 1
}

func show() -> Int {
    do {
        return one()
    } catch _ {
        return 0
    }
}
