#[macro_export]
macro_rules! measure_time {
    ($func:expr) => {{
        let start = std::time::Instant::now();
        let result = $func;
        let duration = start.elapsed();
        let secs = duration.as_secs();
        let millis = duration.subsec_millis();
        let micros = duration.subsec_micros() - millis * 1_000;
        let nanos = duration.subsec_nanos() - millis * 1_000_000 - micros * 1_000;

        let func_name = stringify!($func);

        println!(
            "Time elapsed: {}s {}ms {}µs {}ns [{}]",
            secs, millis, micros, nanos, func_name
        );
        result
    }};
}

#[macro_export]
macro_rules! measure_block_time {
    ($block:block) => {{
        let start = std::time::Instant::now();
        let result = { $block };
        let duration = start.elapsed();
        let secs = duration.as_secs();
        let millis = duration.subsec_millis();
        let micros = duration.subsec_micros() - millis * 1_000;
        let nanos = duration.subsec_nanos() - millis * 1_000_000 - micros * 1_000;

        println!(
            "Time elapsed: {}s {}ms {}µs {}ns",
            secs, millis, micros, nanos
        );
        result
    }};
}