// A non-async, non-throwing function is accepted where an `async` or throwing function type is expected.
enum SaveError implements Error {
    case failed
}

func runSave(save: (String) async throws SaveError -> Bool) {}

func quietSave(text: String) -> Bool {
    return true
}

func run() {
    runSave(save: quietSave)
    runSave(save: { text => true })
}
//@ 13:19-13:28 type: (String) -> Bool
