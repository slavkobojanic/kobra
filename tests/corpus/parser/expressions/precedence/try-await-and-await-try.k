// `try await x` nests AwaitExpr inside TryExpr, and the reverse order nests the other way
let a = try await x
let b = await try x
let c = try try x
