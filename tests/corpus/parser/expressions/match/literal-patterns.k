// integers, negative and float numbers, units, strings, booleans and nil as patterns
func f() {
    match v {
        case 0 { }
        case -1 { }
        case 1.5 { }
        case -2.5 { }
        case 16px { }
        case "héllo" { }
        case true, false { }
        case nil { }
    }
}
