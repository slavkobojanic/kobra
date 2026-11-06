// A use as an expression (a qualified enum case) counts: no K1105.
import k.time.Weekday

let start = Weekday.monday
//          ^^^^^^^ resolves: decl k.time.Weekday
//                  ^^^^^^ resolves: member k.time.Weekday.monday
