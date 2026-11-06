// Another file of the same module uses it.
func eggs(boxes: Int) -> Int {
    return boxes.dozen
    //           ^^^^^ resolves: member Int.dozen
}
