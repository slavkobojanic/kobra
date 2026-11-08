// Each storage key name is declared once in the app (36-local-storage.md), so two services can't
// read each other's data as their own: a second key with the same name is K2405 at its name, with a
// label at the first. Test code may reuse a name to inspect what a service stored.
import k.storage.StorageKey

/// The visitor's todos.
let savedTodos: StorageKey<List<String>> = StorageKey(name: "todos")

/// The visitor's archived todos.
let archivedTodos: StorageKey<List<String>> = StorageKey(name: "archive")
