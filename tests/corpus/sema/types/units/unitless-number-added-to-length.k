// Adding a plain number to a length is K2101: only `*` and `/` take unit-less numbers.
let bad = 16px + 2
let good = 16px * 2
