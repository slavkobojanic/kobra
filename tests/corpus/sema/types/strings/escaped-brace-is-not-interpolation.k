// `{{` is a literal brace: no name inside is resolved, so an unknown word is fine.
let text = "use {{missing} literally"
  //^^^^ type: String
