extern crate i2cdev;
use i2cdev::core::I2CDevice;
use i2cdev::linux::{
    LinuxI2CDevice,
    LinuxI2CError,
};
use std::time::{Duration, Instant};
use std::thread::sleep;

const PN532_ADDR:           u8 = 0x24;

// commands

#[allow(unused)]
enum Command {
    Diagnose	             = 0x00,
    GetFirmwareVersion	     = 0x02,
    GetGeneralStatus	     = 0x04,
    ReadRegister	         = 0x06,
    WriteRegister	         = 0x08,
    ReadGPIO	             = 0x0C,
    WriteGPIO	             = 0x0E,
    SetSerialBaudRate	     = 0x10,
    SetParameters	         = 0x12,
    SAMConfiguration	     = 0x14,
    PowerDown	             = 0x16,
    RFConfiguration	         = 0x32,
    RFRegulationTest	     = 0x58,
    InJumpForDEP	         = 0x56,
    InJumpForPSL	         = 0x46,
    InListPassiveTarget	     = 0x4A,
    InATR	                 = 0x50,
    InPSL	                 = 0x4E,
    InDataExchange	         = 0x40,
    InCommunicateThru	     = 0x42,
    InDeselect	             = 0x44,
    InRelease	             = 0x52,
    InSelect	             = 0x54,
    InAutoPoll	             = 0x60,
    TgInitAsTarget	         = 0x8C,
    TgSetGeneralBytes	     = 0x92,
    TgGetData	             = 0x86,
    TgSetData	             = 0x8E,
    TgSetMetaData	         = 0x94,
    TgGetInitiatorCommand	 = 0x88,
    TgResponseToInitiator	 = 0x90,
    TgGetTargetStatus	     = 0x8A,
}


#[allow(unused)]
enum CardType {
    IsoTypeA  = 0x00,
    FeliCa212 = 0x01,
    FeliCa424 = 0x02,
    IsoTypeB  = 0x03,
    Jewel     = 0x04,
}

pub struct Pn532 {
    i2c: LinuxI2CDevice,
}

impl Pn532 {
    pub fn open(dev: &str) -> Result<Self, LinuxI2CError> {
        let i2c = LinuxI2CDevice::new(dev, PN532_ADDR.into())?;
        Ok(Self {
            i2c
        })
    }

    // information frame, see UM0701-02 page 28
    fn send_frame(&mut self, payload: &[u8]) -> Result<(), LinuxI2CError> {
        assert!(payload.len() < 0xfe);

        let len = payload.len() as u8 + 1;
        let len_checksum = 0u8.wrapping_sub(len);

        // calculating checksum
        let mut checksum = 0u8.wrapping_sub(0xd4);
        for p in payload {
            checksum = checksum.wrapping_sub(*p);
        }

        let mut b = vec![
            0x00, // sync preamble
            0x00, 0xff,  // start
        ];
        b.push(len);
        b.push(len_checksum);
        b.push(0xd4); // direction
        b.extend_from_slice(payload);
        b.push(checksum);
        b.push(0x00); // postamble

        self.i2c.write(&b)
    }

    fn expect_ack(&mut self) -> Result<(), LinuxI2CError> {
        for _ in 0..3{
            sleep(Duration::from_millis(1));
            let mut b = [0u8; 256];
            self.i2c.read(&mut b)?;

            let mut state = 0;
            for i in 0..b.len() {
                match (b[i], state) {
                    (0x00, 0) => {
                        state = 1;
                    }
                    (0x00, 1) => {
                        state = 1;
                    }
                    (0xff, 1) => {
                        state = 2;
                    }
                    (0x00, 2) => {
                        return Ok(());
                    }
                    (0xff, 2) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "nack").into());
                    }
                    (1 , 2) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("application error 0x{:x}", b[i+2])).into())
                    },
                    (_, 2) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "out of order").into());
                    },
                    _ => {
                        state = 0;
                    }
                }
            }
        }
        Err(std::io::Error::new(std::io::ErrorKind::Other, "timeout").into())
    }



    fn receive_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, LinuxI2CError> {
        let now = Instant::now();
        loop {
            if now.elapsed() > timeout {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "timeout").into());
            }
            sleep(Duration::from_millis(1));
            let mut b = [0u8; 256];
            self.i2c.read(&mut b)?;

            let mut state = 0;
            for i in 0..b.len() {
                match (b[i], state) {
                    (0x00, 0) => {
                        state = 1;
                    }
                    (0x00, 1) => {
                        state = 1;
                    }
                    (0xff, 1) => {
                        state = 2;
                    }
                    (0x00, 2) => {
                        // ack frame
                        break;
                    }
                    (0xff, 2) => {
                        // nack frame or extended
                        break;
                    }
                    (1 , 2) => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("application error 0x{:x}", b[i+2])).into())
                    },
                    (size, 2) => {
                        return Ok(b[i+3.. i+3 + (size as usize - 1)].to_vec());
                    },
                    _ => {
                        state = 0;
                    }
                }
            }
        }
    }

    // ( IC version , firmware version, firmware revision, feature bitfield)
    pub fn get_firmware_version(&mut self) -> Result<(u8,u8,u8,u8), LinuxI2CError> {
        self.send_frame(&[Command::GetFirmwareVersion as u8])?;
        self.expect_ack()?;
        let r = self.receive_frame(Duration::from_millis(10))?;
        Ok((r[1],r[2],r[3],r[4]))
    }


    pub fn powerdown(&mut self) -> Result<(), LinuxI2CError> {
        self.send_frame(&[
            Command::PowerDown as u8,
            0b10000011,
        ])?;
        self.expect_ack()?;

        // according to page 98 remarks, we need to lock the bus for 1ms,
        // otherwise the chip might get confused
        sleep(Duration::from_millis(1));

        Ok(())
    }


    pub fn setup(&mut self) -> Result<(), LinuxI2CError> {
        self.send_frame(&[
            Command::SAMConfiguration as u8,
            0x01 // normal mode
        ])?;
        self.expect_ack()?;
        Ok(())
    }

    pub fn list(&mut self, timeout: Duration) -> Result<Vec<Vec<u8>>, LinuxI2CError> {
        self.send_frame(&[
            Command::InListPassiveTarget as u8,
            0x02, // max-targets. the chip only supposed 2, so i dunno why this is a parameter
            CardType::IsoTypeA as u8,
        ])?;
        self.expect_ack()?;




        let r = self.receive_frame(timeout)?;

        if r.len() < 5 {
            return Ok(Vec::new());
        }

        let num = r[1];
        let mut i = 2;

        let mut tags = Vec::new();
        for _ in 0..num {
            if i >= r.len() {
                return Ok(Vec::new());
            }
            i   += 1 // note that the spec is confusingly missing a one byte enumerator prefix
                +  2 // sens_res
                +  1 // sel_res
            ;

            if i >= r.len() {
                return Ok(Vec::new());
            }
            let len     = r[i] as  usize;
            i += 1;
            if i >= r.len() {
                return Ok(Vec::new());
            }
            if i + len  > r.len() {
                return Ok(Vec::new());
            }
            tags.push(r[i .. i + len].to_vec());
            i += len;

            //ats
            if i < r.len() {
                let len = r[i] as  usize;
                // the chip doesn't tell us if there's an ats field, it just emits one or not.
                // in this case the the ats length field will be the index 2, which is also not a valid ats size,
                if len == 2 {
                    continue;
                }
                i += len;
            }
        }

        Ok(tags)
    }
}



