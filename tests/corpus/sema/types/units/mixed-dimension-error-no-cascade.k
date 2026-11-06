// After a K2101, the binding's type is `{error}` and later uses report nothing more.
let bad = 1s + 4px
  //^^^ type: {error}
let worse = bad * 2
