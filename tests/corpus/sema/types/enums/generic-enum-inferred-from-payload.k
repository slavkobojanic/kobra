// A generic enum's type argument is inferred from the payload.
enum Fetch<T> {
    case idle
    case loaded(T)
}

let done = Fetch.loaded(5)
  //^^^^ type: corpus.Fetch<Int>
let names: Fetch<List<String>> = .loaded(["a"])
                               //^^^^^^^^^^^^^^ type: corpus.Fetch<List<String>>
