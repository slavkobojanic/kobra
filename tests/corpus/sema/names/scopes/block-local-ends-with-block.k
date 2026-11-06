// A binding declared in a block is out of scope after the block's `}`.
func pick(flag: Bool) -> Int {
    if flag {
        let inner = 1
    }
    return inner
}
