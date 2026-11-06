// `[]` and `[:]` take their type from the expected type: annotations, parameters and returns.
let names: List<String> = []
                        //^^ type: List<String>
let ages: Map<String, Int> = [:]
                           //^^^ type: Map<String, Int>

func count(items: List<Int>) -> Int {
    return items.count
}

let zero = count(items: [])
                      //^^ type: List<Int>

func none() -> Map<String, Bool> {
    return [:]
         //^^^ type: Map<String, Bool>
}
