// `failed(let e {` is missing `)`: K0101 at the `(`, and the block still parses.
page Product {
    failed(let e {
        text { "x" }
    }
}
