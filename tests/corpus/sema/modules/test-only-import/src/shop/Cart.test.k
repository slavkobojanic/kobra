import testing.Fake

test "uses the fake" {
    let fake = Fake(id: 1)
    expect fake.id == 1
}
