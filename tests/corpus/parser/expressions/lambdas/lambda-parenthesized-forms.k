// parenthesized parameter lists: empty, untyped, typed, trailing comma, return type only
let a = { () => 1 }
let b = { (x, y) => x }
let c = { (x: Int, y: Int) => x + y }
let d = { (x, y,) => y }
let e = { () -> Int => 1 }
let g = { (items: List<Item>, f: (Item) -> Bool) -> Item? => nil }
