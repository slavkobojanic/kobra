// A list of optionals accepts `nil` and values against its annotation.
let slots: List<Int?> = [1, nil, 3]
                          //^^^ type: Int?
let count = slots.count
  //^^^^^ type: Int
