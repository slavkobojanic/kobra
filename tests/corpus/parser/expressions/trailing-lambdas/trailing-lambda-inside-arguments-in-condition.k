// inside an argument list within a condition, trailing lambdas are allowed again
func f() {
    if any(xs.map { x => x.ok }) { go() }
}
