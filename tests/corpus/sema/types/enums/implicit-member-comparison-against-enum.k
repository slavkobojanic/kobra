// In `==`, the other operand's type is the expected type of an implicit member.
enum Size {
    case small
    case large
}

func isSmall(size: Size) -> Bool {
    return size == .small
                  //^^^^^ resolves: member corpus.Size.small
}
