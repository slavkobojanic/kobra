// A local shadows a top-level declaration of its module.
let limit = 10

func capped(size: Int) -> Int {
    let limit = 5
    return size + limit
    //            ^^^^^ resolves: local limit 5:9
}
