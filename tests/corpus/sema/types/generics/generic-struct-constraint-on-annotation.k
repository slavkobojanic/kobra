// A generic struct's `where` clause is checked where the type is written: K2009 at the type argument.
struct Registry<K> where K implements Hashable {
    let keys: List<K>
}

let names: Registry<String> = Registry(keys: [])
let reals: Registry<Double> = Registry(keys: [])
