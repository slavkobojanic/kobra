// A `.` without a member name is a syntax error only: no K1203 for the missing name.
struct Box {
    let length: Int
}

func size(box: Box) -> Int {
    return box.
}
