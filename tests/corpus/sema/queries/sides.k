entity Note {
//     ^^^^ side: server-only
    @Id let id: Int
    let text: String
}

struct Draft {
//     ^^^^^ side: shared
    let text: String
}
