// `..<` and `...` build half-open and closed ranges.
let digits = 0..<10
  //^^^^^^ type: HalfOpenRange<Int>
let dice = 1...6
  //^^^^ type: ClosedRange<Int>
