// An unknown name makes its expression `{error}`; every use of it stays silent.
let total = missing + 1
  //^^^^^ type: {error}
let doubled: Int = total * 2
let label: String = total
