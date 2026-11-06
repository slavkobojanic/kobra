let numbers = [1, 2, 3]
//  ^^^^^^^ type: List<Int>
let names: List<String> = []
//                        ^^ type: List<String>
let maybe: String? = nil
//                   ^^^ type: String?

func twice(value: Int) -> Int {
    return value * 2
    //     ^^^^^^^^^ type: Int
}
//@ 8:6-8:11 type: (Int) -> Int
