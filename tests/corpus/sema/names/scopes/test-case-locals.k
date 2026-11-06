// path: src/corpus/Math.test.k
// A test case's body is a block like any other.
test "locals in a test" {
    let base = 2
    let squared = base * base
    //            ^^^^ resolves: local base 4:9
    expect squared == 4
    //     ^^^^^^^ resolves: local squared 5:9
}
