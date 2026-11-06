// A binding in an inner block shadows an outer one until the block ends.
func pick(flag: Bool) -> Int {
    let value = 1
    if flag {
        let value = 2
        return value
        //     ^^^^^ resolves: local value 5:13
    }
    return value
    //     ^^^^^ resolves: local value 3:9
}
