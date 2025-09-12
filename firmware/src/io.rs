use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    mutex::Mutex,
};
use esp_hal::gpio::{AnyPin, Output};

type LedType = Mutex<CriticalSectionRawMutex, Option<Output<'static>>>;
pub static LED1: LedType = Mutex::new(None);
pub static LED2: LedType = Mutex::new(None);

pub async fn blink_led2_ms(ms: u64) {
    let mut led2 = LED2.lock().await;
    if let Some(led) = led2.as_mut() {
        led.set_high();
        embassy_time::Timer::after(embassy_time::Duration::from_millis(ms)).await;
        led.set_low();
    }
}
