// Durations from extensions and literals combine; negation keeps the type.
let total = 10.minutes + 30.seconds
  //^^^^^ type: Duration
let mixed = 1.second + 500ms
  //^^^^^ type: Duration
let back = -5.minutes
  //^^^^ type: Duration
let secs = 3.weeks.inSeconds
  //^^^^ type: Int
