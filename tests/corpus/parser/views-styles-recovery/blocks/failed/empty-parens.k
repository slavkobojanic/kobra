// `failed()` with nothing in the parentheses is K0124 at the `)`; the block still parses.
page Product {
    failed() {}
}
