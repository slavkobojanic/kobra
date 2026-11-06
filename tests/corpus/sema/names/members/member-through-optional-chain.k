// `?.` resolves the member against the optional's wrapped type.
struct Box {
    let length: Int
}

func size(box: Box?) -> Int {
    return box?.length ?? 0
    //          ^^^^^^ resolves: member corpus.Box.length
}
