// No import names the extension (it has no name), yet its member resolves.
func eggs(boxes: Int) -> Int {
    return boxes.dozen
    //           ^^^^^ resolves: member Int.dozen
}
