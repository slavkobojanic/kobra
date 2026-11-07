// A `StorageKey` is a plain value on the server too: a module that builds
// one at load time (a client service lowered for server rendering, or the
// server half of a test run) loads instead of panicking.
import k.storage.StorageKey

let todosKey: StorageKey<List<String>> = StorageKey(name: "todos")

func main() -> String {
    return todosKey.name
}
