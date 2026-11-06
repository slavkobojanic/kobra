// A helper in a test file is for the module's tests only.
func sampleCart() -> Cart {
    return Cart(size: 1)
}

test "a sample cart has one line" {
    expect sampleCart().size == 1
}
