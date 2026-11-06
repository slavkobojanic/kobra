// `[a, b]` is `List<T>` of its elements' type; nested literals nest.
let numbers = [1, 2, 3]
  //^^^^^^^ type: List<Int>
let grid = [[1], [2, 3]]
  //^^^^ type: List<List<Int>>
let words = ["a", "b"]
  //^^^^^ type: List<String>
