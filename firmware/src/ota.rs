use defmt::{debug, info, warn};
use embassy_time::Duration;

use embassy_net::{tcp::TcpSocket, Stack};
use embedded_io_async::Read;
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;

const RX_BUF_SIZE: usize = 4096;
const TX_BUF_SIZE: usize = 4096;

#[derive(Debug, serde::Deserialize)]
struct OtaHeader {
    flash_size: u32,
    target_crc: u32,
}

/// Runs the OTA update server.
#[embassy_executor::task]
pub async fn listen(stack: Stack<'static>) -> ! {
    let mut rx_buf = [0; RX_BUF_SIZE];
    let mut tx_buf = [0; TX_BUF_SIZE];

    let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    socket.set_timeout(Some(Duration::from_secs(20)));

    let endpoint = embassy_net::IpListenEndpoint {
        addr: None,
        port: 1337,
    };

    socket.accept(endpoint).await.unwrap();

    loop {
        info!("Starting OTA update...");

        let mut header_buf = [0u8; core::mem::size_of::<OtaHeader>()];
        socket.read_exact(&mut header_buf).await.unwrap();

        let header: OtaHeader = postcard::from_bytes(&header_buf).unwrap();

        let mut ota = Ota::new(FlashStorage::new()).unwrap();
        ota.ota_begin(header.flash_size, header.target_crc).unwrap();

        let mut buf = [0; 4096];
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            if n == 0 {
                warn!("Connection closed");
                break;
            }

            let res = ota.ota_write_chunk(&buf[..n]);
            if res == Ok(true) {
                // end of flash
                if ota.ota_flush(true, true).is_ok() {
                    // true if you want to verify crc reading flash, and true if you want rollbacks
                    info!("OTA update complete, restarting...");
                    esp_hal::system::software_reset();
                }
            }

            let progress = (ota.get_ota_progress() * 100.0) as u8;
            info!("progress: {}%", progress);
        }
    }
}
