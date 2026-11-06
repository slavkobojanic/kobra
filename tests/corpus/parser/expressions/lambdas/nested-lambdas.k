// a lambda returning a lambda, and a lambda passed inside a lambda
let add = { a => { b => a + b } }
let m = { xs => xs.map { x => x + 1 } }
