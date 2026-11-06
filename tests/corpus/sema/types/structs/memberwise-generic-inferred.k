// A generic struct's type arguments are inferred from the memberwise arguments.
struct Pair<A, B> {
    let first: A
    let second: B
}

let pair = Pair(first: 1, second: "one")
  //^^^^ type: corpus.Pair<Int, String>
let name = pair.second
  //^^^^ type: String
