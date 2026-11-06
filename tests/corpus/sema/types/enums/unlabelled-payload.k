// An unlabelled payload is passed positionally.
enum Token {
    case number(Int)
    case word(String)
}

let n: Token = .number(42)
                     //^^ type: Int
let w = Token.word("hi")
  //^ type: corpus.Token
