// Module-qualified names, nested type names and generic types stay `Type`s.
test LocalStorageUsers {
    use k.test.MemoryStorage for LocalStorage
    use Outer.Inner for Api
    use InMemoryRepository<User> for Repository<User>
    use (Fake) for Api
}
