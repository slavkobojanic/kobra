// `c ? .a : .b` is a ternary (K0107), not optional chaining: `? .` has a space
let tone = ok ? .success : .danger
