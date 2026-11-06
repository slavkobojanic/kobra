// `[k: v]` is `Map<K, V>`.
let ages = ["Ana": 31, "Ben": 42]
  //^^^^ type: Map<String, Int>
let age = ages.get("Ana")
  //^^^ type: Int?
