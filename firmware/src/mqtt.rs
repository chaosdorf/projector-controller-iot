use defmt::info;
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_println::println;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::publish_packet::QualityOfService,
    utils::rng_generator::CountingRng,
};
use serde::Serialize;

use crate::io::{self, LED1};

#[derive(Serialize)]
struct DiscoveryPacket<'a> {
    unique_id: &'a str,
    name: &'a str,
    state_topic: &'a str,
    command_topic: &'a str,
    availability_topic: &'a str,
    payload_on: &'a str,
    payload_off: &'a str,
    state_on: &'a str,
    state_off: &'a str,
    optimistic: bool,
    qos: u8,
    retain: bool,
}

async fn send_discovery_packet(client: &mut MqttClient<'static, TcpSocket<'_>, 5, CountingRng>) {
    let data = DiscoveryPacket {
        unique_id: "projector-controller",
        name: "Projector Controller",
        state_topic: "projector-controller/state",
        command_topic: "projector-controller/command",
        availability_topic: "projector-controller/availability",
        payload_on: "ON",
        payload_off: "OFF",
        state_on: "ON",
        state_off: "OFF",
        optimistic: false,
        qos: 0,
        retain: true,
    };

    let json = serde_json::to_string(&data).unwrap();
    client
        .send_message(
            "homeassistant/switch/projector-controller/config",
            json.as_bytes(),
            QualityOfService::QoS0,
            true,
        )
        .await
        .unwrap();

    println!("Sent discovery packet: {}", json);

    client
        .send_message(
            "projector-controller/availability",
            b"online",
            QualityOfService::QoS0,
            true,
        )
        .await
        .unwrap()
}

#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>) {
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
            {
                let mut led_unlocked = LED1.lock().await;
                if let Some(pin_ref) = led_unlocked.as_mut() {
                    pin_ref.toggle();
                }
            }
            Timer::after(Duration::from_millis(100)).await;
        }
    }

    println!("Connected to broker!");
    let mut mqtt_config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(20000),
    );
    // mqtt_config
    //     .add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1);
    mqtt_config.add_client_id("projector-controller");
    // config.add_username(USERNAME);
    // config.add_password(PASSWORD);
    // mqtt_config.max_packet_size = 900;

    static mut TX_BUF: [u8; 2048] = [0; 2048];
    static mut RX_BUF: [u8; 2048] = [0; 2048];

    let tx_buf = unsafe { &mut TX_BUF };
    let rx_buf = unsafe { &mut RX_BUF };

    let mut client =
        MqttClient::<'_, _, 5, _>::new(socket, tx_buf, 2048, rx_buf, 2048, mqtt_config);

    client.connect_to_broker().await.unwrap();
    println!("Connected to MQTT server!");

    client
        .subscribe_to_topic("projector-controller/command")
        .await
        .unwrap();
    println!("Subscribed to command topic");

    println!("Subscribed to topic");

    send_discovery_packet(&mut client).await;
    println!("Sent discovery packet");

    loop {
        println!("Waiting for message...");
        let (topic, data) = client.receive_message().await.unwrap();
        defmt::println!("Received on topic {}: {:?}", topic, data);

        // FIXME: do not block for 20ms lolololol
        io::blink_led2_ms(20).await;

        match topic {
            "projector-controller/command" => {
                let msg = core::str::from_utf8(data).unwrap();
                println!("State message: {}", msg);

                // Clone to break the borrow
                let data_owned = data.to_vec();

                client
                    .send_message(
                        "projector-controller/state",
                        &data_owned,
                        QualityOfService::QoS0,
                        true,
                    )
                    .await
                    .unwrap();

                println!("Published state message: {:?}", &data_owned);
            }
            _ => {
                println!("Unknown topic: {}", topic);
            }
        }
    }
}
