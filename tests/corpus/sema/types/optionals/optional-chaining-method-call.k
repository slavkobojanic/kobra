// A method called through `?.` returns an optional of its result.
func shout(text: String?) -> String? {
    let loud = text?.uppercased()
      //^^^^ type: String?
    return loud
}
