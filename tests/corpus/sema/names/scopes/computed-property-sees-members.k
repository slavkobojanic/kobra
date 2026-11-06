// A computed property's body sees the type's members.
struct Box {
    let width: Int
    let height: Int

    var area: Int {
        width * height
      //^^^^^ resolves: member corpus.Box.width
        //      ^^^^^^ resolves: member corpus.Box.height
    }
}
