// Non-mutating collection methods (`appending`, `filter`, `map`) are fine on a `let` list.
func names() -> List<String> {
    let names = ["Ana"]
    let more = names.appending("Ben")
      //^^^^ type: List<String>
    return more.filter { name => name.isNotEmpty }
}
