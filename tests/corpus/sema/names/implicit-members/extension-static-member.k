// A static member added by an extension is an implicit member of the extended type.
extend Color {
    static let brand: Color = #3B82F6
}

let accent: Color = .brand
//                   ^^^^^ resolves: member Color.brand
