use defmt::{debug, error, info, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::xtensa_lx::debug_break;
use esp_println::println;
use heapless::Vec;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::publish_packet::QualityOfService,
    utils::rng_generator::CountingRng,
};
use serde::Serialize;
use serde_json::json;
use serde_json_core::to_slice;

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

/// send Home Assistant MQTT discovery packets and subscribe to command topics
async fn homassistant_initialization(
    client: &mut MqttClient<'static, TcpSocket<'_>, 5, CountingRng>,
) {
    // what. in. the. actual. fuck.
    // why does this need *serde_json_core::heapless::Vec* instead of heapless::Vec??????
    let mut topics = serde_json_core::heapless::Vec::<&str, 16>::new();

    // Power switch
    let power = json!({
        "name": "Projector Power",
        "unique_id": "projector_power",
        "command_topic": "projector-controller/cmd/power",
        "state_topic": "projector-controller/stat/power",
        "availability_topic": "projector-controller/availability",
        "payload_on": "ON",
        "payload_off": "OFF",
        "state_on": "ON",
        "state_off": "OFF",
        "optimistic": false
    });
    publish_config(
        client,
        "homeassistant/switch/projector_power/config",
        &power,
    )
    .await;

    topics.push("projector-controller/cmd/power").unwrap();

    debug!("Published power config");

    // Projector control buttons (all high-level, no RS232 codes here)
    let buttons: &[(&str, &str)] = &[
        ("menu", "Menu"),
        ("enter", "Enter"),
        ("up", "Up"),
        ("down", "Down"),
        ("left", "Left"),
        ("right", "Right"),
        ("back", "Back"),
    ];

    for (id, name) in buttons {
        let data = json!({
            "name": alloc::format!("Projector {}", name), // compile-time friendly
            "unique_id": alloc::format!("projector_{}", id),
            "command_topic": alloc::format!("projector-controller/cmd/{}", id),
            "availability_topic": "projector-controller/availability",
        });

        let topic = match *id {
            "menu" => "homeassistant/button/projector_menu/config",
            "enter" => "homeassistant/button/projector_enter/config",
            "up" => "homeassistant/button/projector_up/config",
            "down" => "homeassistant/button/projector_down/config",
            "left" => "homeassistant/button/projector_left/config",
            "right" => "homeassistant/button/projector_right/config",
            "back" => "homeassistant/button/projector_back/config",
            _ => continue,
        };

        debug!("Publishing {} config (data: {})", id, data.as_str());

        publish_config(client, topic, &data).await;

        debug!("Published {} config", id);
    }

    topics.push("projector-controller/cmd/menu").unwrap();
    topics.push("projector-controller/cmd/enter").unwrap();
    topics.push("projector-controller/cmd/up").unwrap();
    topics.push("projector-controller/cmd/down").unwrap();
    topics.push("projector-controller/cmd/left").unwrap();
    topics.push("projector-controller/cmd/right").unwrap();
    topics.push("projector-controller/cmd/back").unwrap();

    // Binary sensor for actual power state
    let status = json!({
        "name": "Projector Status",
        "unique_id": "projector_status",
        "state_topic": "projector-controller/stat/status",
        "payload_on": "ON",
        "payload_off": "OFF",
        "availability_topic": "projector-controller/availability"
    });
    publish_config(
        client,
        "homeassistant/binary_sensor/projector_status/config",
        &status,
    )
    .await;

    debug!("Published status config");

    // Device availability
    client
        .send_message(
            "projector-controller/availability",
            b"online",
            QualityOfService::QoS0,
            true,
        )
        .await
        .unwrap();

    debug!("Published availability online");

    // Subscribe to command topics
    client.subscribe_to_topics(&topics).await.unwrap();

    debug!("Subscribed to topics ({=[?]})", &topics);
}

/// Serialize JSON into fixed buffer and publish (no alloc, no format!)
async fn publish_config(
    client: &mut MqttClient<'static, TcpSocket<'_>, 5, CountingRng>,
    topic: &str,
    data: &serde_json::Value,
) {
    let mut buf = [0u8; 512]; // adjust if JSON grows
    let used = to_slice(data, &mut buf).unwrap();
    client
        .send_message(topic, &buf[..used], QualityOfService::QoS0, true)
        .await
        .unwrap();
}

#[embassy_executor::task]
pub async fn mqtt_task(stack: &'static Stack<'static>) {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    let mut socket = TcpSocket::new(*stack, &mut rx_buffer, &mut tx_buffer);

    socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

    let broker_addr = stack
        .dns_query(env!("MQTT_BROKER"), smoltcp::wire::DnsQueryType::A)
        .await
        .unwrap();
    let broker_endpoint = (broker_addr[0], 1883);

    info!("Connecting to broker...");

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

    info!("Connected to broker!");
    let mut mqtt_config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(20000),
    );
    // mqtt_config
    //     .add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0);
    mqtt_config.add_client_id("projector-controller");
    // config.add_username(USERNAME);
    // config.add_password(PASSWORD);
    // mqtt_config.max_packet_size = 900;

    // this has to be weird and unsafe so client can be passed around (?)
    static mut TX_BUF: [u8; 4096] = [0; 4096];
    static mut RX_BUF: [u8; 4096] = [0; 4096];

    let tx_buf = unsafe { &mut TX_BUF };
    let rx_buf = unsafe { &mut RX_BUF };

    let mut client =
        MqttClient::<'_, _, 5, _>::new(socket, tx_buf, 4096, rx_buf, 4096, mqtt_config);

    client.connect_to_broker().await.unwrap();
    info!("Connected to MQTT server!");

    homassistant_initialization(&mut client).await;
    info!("Sent discovery packet");

    let mut projector = io::PROJECTOR.lock().await;
    let projector = projector.as_mut().unwrap();

    loop {
        match select(client.receive_message(), Timer::after_secs(2)).await {
            Either::First(msg) => {
                let (topic, data) = msg.unwrap();
                info!("Received on topic {}: {:?}", topic, data);

                // FIXME: do not block for 20ms lolololol
                io::blink_led2_ms(20).await;

                match topic {
                    "projector-controller/cmd/power" => {
                        let msg = core::str::from_utf8(data).unwrap();
                        println!("State message: {}", msg);

                        // Clone to break the borrow
                        let data_owned = data.to_vec();

                        match msg {
                            "ON" => {
                                if let Err(_) = projector.power_on() {
                                    error!("Failed to send power on command");
                                    continue;
                                } else {
                                    info!("Sent power on command");
                                }
                            }
                            "OFF" => {
                                if let Err(_) = projector.power_off() {
                                    error!("Failed to send power off command");
                                    continue;
                                } else {
                                    info!("Sent power off command");
                                }
                            }
                            _ => {
                                warn!("Unknown power command: {}", msg);
                            }
                        }

                        client
                            .send_message(
                                "projector-controller/stat/power",
                                &data_owned,
                                QualityOfService::QoS0,
                                true,
                            )
                            .await
                            .unwrap();

                        info!("Published state message: {=[?]}", &data_owned);
                    }
                    _ => {
                        info!("Unknown topic: {}", topic);
                    }
                }
            }
            Either::Second(()) => {
                // periodically send availability
                client
                    .send_message(
                        "projector-controller/availability",
                        b"online",
                        QualityOfService::QoS0,
                        true,
                    )
                    .await
                    .unwrap();
                debug!("Published availability online");
            }
        }
    }
}
