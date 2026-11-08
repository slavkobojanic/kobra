import k.storage.StorageKey

test Keys {
    test "a test may name an existing key" {
        let stored: StorageKey<List<String>> = StorageKey(name: "todos")
        expect stored.name == "todos"
    }
}
