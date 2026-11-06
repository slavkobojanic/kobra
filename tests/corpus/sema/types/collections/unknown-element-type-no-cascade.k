// An element that fails to resolve doesn't make the list literal report K2001 for its other elements.
let items = [1, missing, 3]
  //^^^^^ type: List<Int>
