// further trailing lambdas are chained through members: `list(items) { ... }.row { ... }`
let a = table(rows) { r => cell(r) }.header { title }
