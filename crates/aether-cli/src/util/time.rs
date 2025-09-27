use std::time::Duration;
pub fn fmt_duration(d: Duration) -> String { format!("{}ms", d.as_millis()) }
