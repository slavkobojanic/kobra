// Test-file declarations don't leave their module, not even for another module's tests: K1201, and nothing more.
test "another module's fixture" {
    let fixture = Fixture(id: 1)
    expect fixture.id == 1
}
