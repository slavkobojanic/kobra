// Number extensions on `Int` produce `Duration`; singular forms are aliases.
let timeout = 10.minutes
  //^^^^^^^ type: Duration
               //^^^^^^^ resolves: member Int.minutes
let one = 1.hour
  //^^^ type: Duration
let week = 2.weeks
  //^^^^ type: Duration
let tiny = 5.milliseconds
  //^^^^ type: Duration
