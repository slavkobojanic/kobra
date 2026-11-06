// A test-file helper of `shop`.
struct Fixture {
    let id: Int
}

test "a fixture has an id" {
    expect Fixture(id: 1).id == 1
}
