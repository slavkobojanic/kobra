func pick(flag: Bool) -> Int {
    let x: Int
    if flag {
        x = 1
    }
    return x
}

func pickBoth(flag: Bool) -> Int {
    let x: Int
    if flag {
        x = 1
    } else {
        x = 2
    }
    return x
}
