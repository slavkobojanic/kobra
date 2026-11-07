// An error thrown deep in a call chain propagates through every `try` until a `catch` handles it; the code after each throwing call doesn't run.
import k.observe.Log

enum StockError implements Error {
    case soldOut
}

func reserve(item: String) throws StockError -> Int {
    if item == "mug" {
        throw StockError.soldOut
    }
    return 1
}

func order(item: String) throws StockError -> Int {
    Log.info("ordering {item}")
    let n = try reserve(item: item)
    Log.info("reserved {item}")
    return n
}

func main() -> List<String> {
    var out: List<String> = []
    for item in ["cup", "mug"] {
        do {
            let n = try order(item: item)
            out.append("{item}: {n}")
        } catch .soldOut {
            out.append("{item}: sold out")
        }
    }
    return out
}
