/// Two `@State` lists and an `if` whose branches each mutate one of them:
/// only the branch taken may run, copy-on-write work included (an `if`
/// with single-call branches used to be lowered to a conditional
/// expression whose branches' state writes were hoisted out of it).
public service Shelves {
    @State var left: List<Int> = [1, 2, 3]
    @State var right: List<Int> = []

    /// Moves the first book of one shelf to the end of the other.
    func move(fromLeft: Bool) {
        let book = if fromLeft {
            left.remove(at: 0)
        } else {
            right.remove(at: 0)
        }

        if fromLeft {
            right.append(book)
        } else {
            left.append(book)
        }
    }
}
