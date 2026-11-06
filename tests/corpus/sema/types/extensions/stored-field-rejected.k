// Extensions can't add stored fields: K2016 at the field's name.
struct Point {
    let x: Int
}

extend Point {
    var label: String = ""
    var double: Int { x * 2 }
}
