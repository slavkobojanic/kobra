// `c ? a : b` is K0107; the whole ternary is wrapped in an ErrorNode whose operands are still parsed
let label = done ? "Done" : "Pending"
