#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![feature(type_alias_impl_trait)]
// #![warn(missing_docs)]

use alloc::string::ToString;
use defmt::{debug, error, info, warn};
use embassy_executor::Spawner;
use embassy_net::{Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Output, OutputConfig};
use esp_hal::rng::Rng;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::StopBits;
use esp_println as _;
use esp_wifi::EspWifiController;
use static_cell::make_static;

use crate::projector::Projector;

mod io;
mod log;
mod mqtt;
mod net;
mod ota;
mod projector;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default();
    let peripherals = esp_hal::init(config);

    // GPIOs
    let led1 = Output::new(
        peripherals.GPIO6,
        esp_hal::gpio::Level::Low,
        OutputConfig::default(),
    );

    let led2 = Output::new(
        peripherals.GPIO8,
        esp_hal::gpio::Level::Low,
        OutputConfig::default(),
    );

    {
        *(io::LED1.lock().await) = Some(led1);
        *(io::LED2.lock().await) = Some(led2);
    }

    esp_alloc::heap_allocator!(size: 64 * 1024);

    ///////////////////////////////////////////////////////////////////////////
    // PT-AH1000E
    ///////////////////////////////////////////////////////////////////////////

    // UART1
    let uart_conf = esp_hal::uart::Config::default()
        .with_baudrate(9600)
        .with_stop_bits(StopBits::_1);

    let uart1 = esp_hal::uart::Uart::new(peripherals.UART1, uart_conf).unwrap();

    let projector = Projector::new(uart1);

    {
        *(io::PROJECTOR.lock().await) = Some(projector);
    }

    ////////////////////////////////////////////////////////////////////////////
    // WIFI and NETWORKING
    ////////////////////////////////////////////////////////////////////////////
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg0.timer0, rng.clone()).unwrap()
    );

    io::test_leds().await;

    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, peripherals.WIFI).unwrap();

    let wifi_interface = interfaces.sta;

    let config = embassy_net::Config::dhcpv4(Default::default());

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    let stack: &'static Stack<'static> = make_static!(stack);

    spawner.spawn(net::connection(controller)).ok();
    spawner.spawn(net::net_task(runner)).ok();

    loop {
        if stack.is_link_up() {
            let mut led_unlocked = io::LED1.lock().await;
            if let Some(pin_ref) = led_unlocked.as_mut() {
                pin_ref.set_low();
            }
            break;
        }
        {
            let mut led_unlocked = io::LED1.lock().await;
            if let Some(pin_ref) = led_unlocked.as_mut() {
                pin_ref.toggle();
            }
            Timer::after(Duration::from_millis(100)).await;
            if let Some(pin_ref) = led_unlocked.as_mut() {
                pin_ref.toggle();
            }
            Timer::after(Duration::from_millis(100)).await;
        }
    }

    info!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address.to_string().as_str());
            break;
        }
        {
            let mut led_unlocked = io::LED1.lock().await;
            if let Some(pin_ref) = led_unlocked.as_mut() {
                pin_ref.toggle();
            }
            Timer::after(Duration::from_millis(200)).await;
            if let Some(pin_ref) = led_unlocked.as_mut() {
                pin_ref.toggle();
            }
            Timer::after(Duration::from_millis(200)).await;
        }
    }

    spawner.spawn(ota::listen(&stack)).unwrap();
    spawner.spawn(mqtt::mqtt_task(&stack)).unwrap();

    let _ = spawner;

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
