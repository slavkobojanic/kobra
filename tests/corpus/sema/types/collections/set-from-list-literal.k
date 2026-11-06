// A list literal against an expected `Set<T>` builds a set; `Set(list)` does too.
let tags: Set<String> = ["new", "sale"]
                      //^^^^^^^^^^^^^^^ type: Set<String>
let more = Set(["a", "b"])
  //^^^^ type: Set<String>
let has = tags.contains("new")
  //^^^ type: Bool
