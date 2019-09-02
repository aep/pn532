implementation of the PN532 I2C spec in rust







useage:

```rust
use nfc::Pn532;

pub fn main() {
    let mut nfc = Pn532::open("/dev/i2c-0").unwrap();
    let fwv = nfc.get_firmware_version().unwrap();
    println!("nfc firmware version: {}.{}", fwv.1, fwv.2);

    nfc.setup().unwrap();
    println!("{:x?}", nfc.list(std::time::Duration::from_secs(1)).unwrap());
    nfc.powerdown().unwrap();
}

```
