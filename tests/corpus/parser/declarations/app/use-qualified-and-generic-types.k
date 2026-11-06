// Both sides of `use` are types: qualified, generic or both.
app A {
    use fakes.FakeCartApi for shop.CartProtocol
    use InMemoryRepository<User> for Repository<User> in .test
}
