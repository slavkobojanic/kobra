/// `word`, louder and twice. Its local is a JavaScript reserved word, so the
/// generated code spells it `arguments$` and the source map names it.
func shout(word: String) -> String {
    let arguments = word.uppercased()
    return "{arguments}, {arguments}"
}
