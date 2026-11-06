// Implicit members resolve against a parameter's type.
enum Size {
    case small
    case large
}

func price(size: Size) -> Int {
    return 1
}

let p = price(size: .large)
                   //^^^^^ resolves: member corpus.Size.large
let q = price(size: .huge)
