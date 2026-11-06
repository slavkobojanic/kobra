// After K2006, the binding still has the expected type and later uses are fine.
enum Size {
    case small
}

let s: Size = .medium
  //^ type: corpus.Size
let t: Size = s
