// Mixed length units (and percentages with lengths) are a `Length` kept as a `calc()` term, not an error.
let width = 100px + 2rem
  //^^^^^ type: Length
let fill = 100% - 2rem
  //^^^^ type: Length
let tall = 50vh - 4em
  //^^^^ type: Length
