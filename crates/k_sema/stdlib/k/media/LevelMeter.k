/// A bar showing an [`AudioMeter`]'s level and peak, as `role="meter"`
/// whose value a screen reader hears at most once a second:
/// `LevelMeter(meter: meter, label: "Microphone level")`.
@Native
public component LevelMeter {
    /// The meter to show.
    @Prop let meter: AudioMeter
    /// The accessible name.
    @Prop let label: String
}
