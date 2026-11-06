// each bad construct is reported and none hides the errors after it
func f() {
    let a = b && c
    let d = !e
    let g = h || i
    let j = k ? l : m
    let n = 0..10
    let o = p < q < r
    let ok = 1
}
