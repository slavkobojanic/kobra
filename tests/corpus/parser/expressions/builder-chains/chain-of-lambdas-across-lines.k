// collection pipelines with one trailing-lambda call per line
func f() {
    let names = products
        .filter { product => product.stock > 0 }
        .map { product => product.name }
        .sorted()
}
