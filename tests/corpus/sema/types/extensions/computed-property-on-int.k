// An extension on `Int` adds a computed property; `self` is the number.
struct RetryPolicy {
    let attempts: Int
}

extend Int {
    var retries: RetryPolicy { RetryPolicy(attempts: self) }
}

let policy = 3.retries
  //^^^^^^ type: corpus.RetryPolicy
             //^^^^^^^ resolves: member Int.retries
