use defmt::debug;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_io::Write;
use esp_hal::uart::Uart;

#[derive(Debug)]
pub enum ProjectorError {
    WriteError,
    ParseError,
    ReadError,
}

/// PT-AH1000E Projector Control via RS232
pub struct Projector<'a, Dm: esp_hal::DriverMode> {
    port: Uart<'a, Dm>,
}

impl<'a, Dm: esp_hal::DriverMode> Projector<'a, Dm> {
    //! Create a new Projector instance with the given UART port
    pub fn new(port: Uart<'a, Dm>) -> Self {
        Self { port }
    }

    //
    fn send(&mut self, data: &[u8]) -> Result<(), ProjectorError> {
        debug!(
            "Sending: {:?}",
            core::str::from_utf8(data).unwrap_or("<invalid utf8>")
        );

        self.port
            .write(data)
            .map_err(|_| ProjectorError::WriteError)
            .map(|_| ())
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, ProjectorError> {
        let mut count = 0;

        // read until \r or buffer full
        let mut tmp_buf = [0u8; 1];

        loop {
            match self.port.read(&mut tmp_buf) {
                Ok(_) => {
                    if count >= buffer.len() {
                        break;
                    }
                    buffer[count] = tmp_buf[0];
                    count += 1;

                    // EOL
                    if tmp_buf[0] == b'\r' {
                        break;
                    }
                }
                Err(_) => break, // no more data
            }
        }

        debug!(
            "Received: {:?}",
            core::str::from_utf8(&buffer[..count]).unwrap_or("<invalid utf8>")
        );

        Ok(count)
    }

    pub fn power_on(&mut self) -> Result<(), ProjectorError> {
        self.send(b"PON\r")
    }

    pub fn power_off(&mut self) -> Result<(), ProjectorError> {
        self.send(b"POF\r")
    }

    pub fn menu(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OMN\r")
    }

    pub fn enter(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OEN\r")
    }

    pub fn up(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OBK\r")
    }

    pub fn left(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OCL\r")
    }

    pub fn right(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OCR\r")
    }

    pub fn down(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OCU\r")
    }

    pub fn back(&mut self) -> Result<(), ProjectorError> {
        self.send(b"OCD\r")
    }

    pub fn is_on(&mut self) -> Result<bool, ProjectorError> {
        let mut buffer = [0u8; 16];
        self.send(b"QPW\r")?;
        let len = self.receive(&mut buffer)?;
        let response = core::str::from_utf8(&buffer[..len]).unwrap_or("");

        match response.trim() {
            "000" => Ok(true),
            "001" => Ok(false),
            _ => Err(ProjectorError::ParseError),
        }
    }
}
