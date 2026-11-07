// Each Log level writes its own level name.
import k.observe.Log

func main() {
    Log.debug("d")
    Log.info("i")
    Log.warning("w")
    Log.error("e")
}
