// A local is visible from its declaration to the end of its block.
func area(width: Int) -> Int {
    let height = width * 2
    var total = width * height
    //                  ^^^^^^ resolves: local height 3:9
    total += 1
    return total
    //     ^^^^^ resolves: local total 4:9
}
