// arithmetic and ranges bind tighter than `??`
let a = x ?? y + 1
let b = x * 2 ?? y
let c = r ?? 0..<n
