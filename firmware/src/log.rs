#[export_name = "_esp_println_timestamp"]
fn esp_println_timestamp() -> u64 {
    embassy_time::Instant::now().as_micros() as u64 / 1000
}
