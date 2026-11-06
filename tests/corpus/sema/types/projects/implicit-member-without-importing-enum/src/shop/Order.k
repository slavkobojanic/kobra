// Implicit members resolve against the expected type without importing it.
import catalog.Shirt

let shirt = Shirt(size: .large)
                       //^^^^^ resolves: member catalog.Size.large
let wrong = Shirt(size: .huge)
