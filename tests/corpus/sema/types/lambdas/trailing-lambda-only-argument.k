// A call with only a trailing lambda needs no parentheses.
func measure(work: () -> Int) -> Int {
    return work()
}

let m = measure { 5 }
  //^ type: Int
