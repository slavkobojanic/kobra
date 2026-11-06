// In a chain, only the first unknown member is reported; the rest of the chain is `{error}`.
struct Box {
    let length: Int
}

func size(box: Box) -> Int {
    return box.lenght.value.other
}
