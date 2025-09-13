use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_io::Write;
use esp_hal::uart::Uart;

enum ProjectorError {
    WriteError,
}

pub struct Projector<'a, Dm: esp_hal::DriverMode> {
    port: Uart<'a, Dm>,
}

impl<'a, Dm: esp_hal::DriverMode> Projector<'a, Dm> {
    pub fn new(port: Uart<'a, Dm>) -> Self {
        Self { port }
    }

    pub fn send(&mut self, data: &[u8]) -> Result<(), ProjectorError> {
        self.port
            .write(data)
            .map_err(|_| ProjectorError::WriteError)
            .map(|_| ())
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
