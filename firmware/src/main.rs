#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Output, OutputConfig};
use esp_hal::rng::Rng;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_wifi::EspWifiController;
use rust_mqtt::client::client::MqttClient;
use rust_mqtt::client::client_config::ClientConfig;
use rust_mqtt::utils::rng_generator::CountingRng;

// mod mqtt;
mod log;
mod net;

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

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // GPIOs
    let mut led = Output::new(
        peripherals.GPIO8,
        esp_hal::gpio::Level::Low,
        OutputConfig::default(),
    );

    esp_alloc::heap_allocator!(size: 64 * 1024);

    // WIFI
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg0.timer0, rng.clone()).unwrap()
    );

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

    spawner.spawn(net::connection(controller)).ok();
    spawner.spawn(net::net_task(runner)).ok();

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
    }

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

    let broker_addr = stack
        .dns_query(env!("MQTT_BROKER"), smoltcp::wire::DnsQueryType::A)
        .await
        .unwrap();
    let broker_endpoint = (broker_addr[0], 1883);

    println!("Connecting to broker...");

    let r = socket.connect(broker_endpoint).await;
    if let Err(e) = r {
        println!("Failed to connect to broker: {:?}", e);

        loop {
            led.toggle();
            Timer::after(Duration::from_millis(100)).await;
        }
    }

    println!("Connected to broker!");

    let mut mqtt_config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(20000),
    );
    mqtt_config
        .add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1);
    mqtt_config.add_client_id("projector-controller");
    // config.add_username(USERNAME);
    // config.add_password(PASSWORD);
    mqtt_config.max_packet_size = 100;

    let mut mqtt_rx_buf = [0; 80];
    let mut mqtt_tx_buf = [0; 80];

    let mut client = MqttClient::<_, 5, _>::new(
        socket,
        &mut mqtt_tx_buf,
        80,
        &mut mqtt_rx_buf,
        80,
        mqtt_config,
    );

    client.connect_to_broker().await.unwrap();
    println!("Connected to MQTT broker");

    client
        .subscribe_to_topic("projector-controller/#")
        .await
        .unwrap();

    loop {
        let (topic, data) = client.receive_message().await.unwrap();
        defmt::println!("Received on topic {}: {:?}", topic, data);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
