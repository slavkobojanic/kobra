// Two enums with the same case names are distinct types.
enum Size {
    case small
}

enum Weight {
    case small
}

let s: Size = Weight.small
