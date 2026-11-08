// path: src/corpus/OpeningHours.k
// A TimeOfDay is its `HH:MM` text on both sides (35-time.md, "TimeOfDay"): a literal is a string
// constant, `hour` and `minute` read the text, and nothing calls a TimeOfDay native at run time.
import k.time.TimeOfDay

public component OpeningHours {
    @Prop let opens: TimeOfDay = TimeOfDay("09:30")

    view {
        text { "Opens at {opens}, hour {opens.hour}, minute {opens.minute}" }
    }
}
