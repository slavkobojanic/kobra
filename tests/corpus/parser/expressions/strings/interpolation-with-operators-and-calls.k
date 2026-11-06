// interpolations hold full expressions: arithmetic, calls with labels, indexes, optional chaining
let a = "Total: {price * quantity + tax}"
let b = "{format(amount, currency: "EUR")}"
let c = "First: {xs[0].name}"
let d = "Hi {user?.name ?? "guest"}"
