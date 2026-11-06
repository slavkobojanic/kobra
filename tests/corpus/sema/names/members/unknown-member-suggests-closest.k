// An unknown member is K1203 at the name after `.`, with the closest member as a fix.
struct Box {
    let length: Int
}

func size(box: Box) -> Int {
    return box.lenght
}
