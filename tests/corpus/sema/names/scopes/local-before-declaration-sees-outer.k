// Before a local's declaration, its name still resolves to the outer binding.
func area(height: Int) -> Int {
    if height > 0 {
        let scaled = height * 2
        //           ^^^^^^ resolves: param height 2:11
        let height = scaled
        return height
        //     ^^^^^^ resolves: local height 6:13
    }
    return height
    //     ^^^^^^ resolves: param height 2:11
}
