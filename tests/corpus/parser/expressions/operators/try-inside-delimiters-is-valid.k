// `try` and `await` may start an argument, a list element or a parenthesized expression
let a = f(try g())
let b = [try g(), await h()]
let c = 1 + (try g())
let d = f(x: try await g())
