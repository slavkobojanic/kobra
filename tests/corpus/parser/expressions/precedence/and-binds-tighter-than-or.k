// `and` binds tighter than `or`; both associate to the left
let a = x or y and z
let b = x and y or z
let c = x or y or z
let d = x and y and z
