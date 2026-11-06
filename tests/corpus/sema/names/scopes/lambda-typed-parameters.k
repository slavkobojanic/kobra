// The parameters of a parenthesized lambda list are locals of its body.
let add = { (left: Int, right: Int) -> Int => left + right }
//                                            ^^^^ resolves: local left 2:14
//                                                   ^^^^^ resolves: local right 2:25
