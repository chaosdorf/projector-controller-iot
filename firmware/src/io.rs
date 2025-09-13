use crate::projector::Projector;
use embassy_sync::{
    blocking_mutex::{self, raw::CriticalSectionRawMutex},
    mutex::Mutex,
};
use embassy_time::{Duration, Timer};
use esp_hal::{
    gpio::{AnyPin, Output},
    Async, Blocking,
};

type LedType = Mutex<CriticalSectionRawMutex, Option<Output<'static>>>;
pub static LED1: LedType = Mutex::new(None);
pub static LED2: LedType = Mutex::new(None);

pub static PROJECTOR: Mutex<CriticalSectionRawMutex, Option<Projector<'static, Blocking>>> =
    Mutex::new(None);

pub async fn blink_led2_ms(ms: u64) {
    let mut led2 = LED2.lock().await;
    if let Some(led) = led2.as_mut() {
        led.set_high();
        embassy_time::Timer::after(embassy_time::Duration::from_millis(ms)).await;
        led.set_low();
    }
}

pub async fn test_leds() {
    let mut led1_unlocked = LED1.lock().await;
    let mut led2_unlocked = LED2.lock().await;
    if let Some(pin_ref) = led1_unlocked.as_mut() {
        pin_ref.set_high();
    }

    if let Some(pin_ref) = led2_unlocked.as_mut() {
        pin_ref.set_low();
    }

    for _i in 0..5 {
        if let Some(pin_ref) = led1_unlocked.as_mut() {
            pin_ref.toggle();
        }
        if let Some(pin_ref) = led2_unlocked.as_mut() {
            pin_ref.toggle();
        }
        Timer::after(Duration::from_millis(100)).await;
    }

    if let Some(pin_ref) = led1_unlocked.as_mut() {
        pin_ref.set_low();
    }
    if let Some(pin_ref) = led2_unlocked.as_mut() {
        pin_ref.set_low();
    }
}
