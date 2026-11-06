// Two bindings of one class instance share it; both have the class type.
class Counter {
    var count: Int = 0
}

func shared() -> Int {
    let a = Counter()
    let b = a
      //^ type: corpus.Counter
    b.count = 5
    return a.count
}
