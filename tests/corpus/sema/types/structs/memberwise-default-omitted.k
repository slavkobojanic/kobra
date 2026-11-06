// A field with a default value may be omitted and takes its default; it may also be given.
struct Settings {
    let name: String
    var retries: Int = 3
    var verbose: Bool = false
}

let defaults = Settings(name: "a")
  //^^^^^^^^ type: corpus.Settings
let some = Settings(name: "b", verbose: true)
let every = Settings(name: "c", retries: 5, verbose: true)
                                       //^ type: Int
