// an `expect` expression is a full expression, so trailing lambdas are allowed
func f() {
    expect items.all { i => i.price > 0 }
}
