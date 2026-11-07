import k.validation.Range

/// The job worker's settings.
public config JobConfig {
    /// How many jobs run concurrently per process, `1...256`.
    @Range(1...256) @Env("K_JOB_CONCURRENCY") let concurrency: Int = 4
}
