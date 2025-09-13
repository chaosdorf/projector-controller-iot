use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_io::Write;
use esp_hal::uart::Uart;

enum ProjectorError {
    WriteError,
    ParseError,
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
        self.port
            .write(data)
            .map_err(|_| ProjectorError::WriteError)
            .map(|_| ())
    }

    fn receive(&mut self, buffer: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buffer.iter_mut() {
            match self.port.read() {
                Ok(b) => {
                    *byte = b;
                    count += 1;
                }
                Err(_) => break,
            }
        }
        count
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
        let len = self.receive(&mut buffer);
        let response = core::str::from_utf8(&buffer[..len]).unwrap_or("");

        match response.trim() {
            "000" => Ok(true),
            "001" => Ok(false),
            _ => Err(ProjectorError::ParseError),
        }
    }
}
