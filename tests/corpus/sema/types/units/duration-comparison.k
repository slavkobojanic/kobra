// `Duration` is `Comparable` and `Equatable`; literals and number extensions compare.
let quick = 200ms < 2s
  //^^^^^ type: Bool
let same = 200ms == 0.2.seconds
  //^^^^ type: Bool
