// Inside an extension, the extended type's members resolve unqualified, as in its own body.
struct Box {
    let length: Int
}

extend Box {
    var doubled: Int {
        length * 2
      //^^^^^^ resolves: member corpus.Box.length
    }
}
