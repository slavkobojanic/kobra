// Using a payload case without its arguments where the enum is expected is K2003 just after the case name.
enum Shape {
    case circle(radius: Double)
    case point
}

let p: Shape = .point
let c: Shape = .circle
