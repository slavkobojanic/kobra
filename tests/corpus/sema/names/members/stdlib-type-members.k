// Members of stdlib values resolve to their stdlib declarations; a misspelled one is K1203.
func size(text: String) -> Int {
    return text.count + text.lenght
    //          ^^^^^ resolves: member String.count
}
