use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use esp_hal::uart::Uart;

struct Projector<'a, Dm: esp_hal::DriverMode> {
    port: Mutex<CriticalSectionRawMutex, Uart<'a, Dm>>,
}

impl<'a, Dm: esp_hal::DriverMode> Projector<'a, Dm> {
    pub fn new(port: Uart<'a, Dm>) -> Self {
        Self {
            port: Mutex::new(port),
        }
    }

    pub fn send(&mut self, data: &[u8]) {
        for &b in data {
            nb::block!(self.port.write(b)).ok();
        }
    }

    pub fn receive(&mut self, buffer: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buffer.iter_mut() {
            match nb::block!(self.port.read()) {
                Ok(b) => {
                    *byte = b;
                    count += 1;
                }
                Err(_) => break,
            }
        }
        count
    }
}
