// A struct without stored fields has an empty memberwise initializer.
struct Marker {}

let marker = Marker()
  //^^^^^^ type: corpus.Marker
