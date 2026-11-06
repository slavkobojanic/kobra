// `for await` over a `Stream<T>` in an async function binds each element
// with type `T`.
struct NoteChange {
    let id: Int
}

func follow(changes: Stream<NoteChange>) async -> Int {
    var last = 0
    for await change in changes {
//            ^^^^^^ type: corpus.NoteChange
        last = change.id
    }
    return last
}
