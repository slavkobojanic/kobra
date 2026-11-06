// A `for` variable shadows a parameter of the same name inside the loop.
func last(item: Int, items: List<Int>) -> Int {
    var result = item
    //           ^^^^ resolves: param item 2:11
    for item in items {
        result = item
        //       ^^^^ resolves: local item 5:9
    }
    return result
}
