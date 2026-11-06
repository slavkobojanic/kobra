// Same-unit arithmetic stays in the unit type; numbers scale units from either side.
let gutter = 16px * 2
  //^^^^^^ type: Length
let twice = 2 * 8px
  //^^^^^ type: Length
let sum = 8px + 8px
  //^^^ type: Length
let third = 30px / 3
  //^^^^^ type: Length
let wait = 200ms + 2s
  //^^^^ type: Duration
let negative = -4px
  //^^^^^^^^ type: Length
