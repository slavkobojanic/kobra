// A test file's own declarations are layered over the module's: reusing a name is not a K1004, and inside the test files the test declaration wins.
struct Sample {
    let label: String
}

test "the test file's declaration wins" {
    expect Sample(label: "x").label == "x"
}
