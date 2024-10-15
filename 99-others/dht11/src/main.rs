use dht_sensor::{dht11, DhtReading};
use esp_idf_svc::hal::{delay::{self, FreeRtos}, gpio, prelude::Peripherals};
use log::{error,info};

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals =  Peripherals::take().unwrap();
    let pin = peripherals.pins.gpio3;
    let mut sensor = gpio::PinDriver::input_output(pin).unwrap();

    sensor.set_high().unwrap();

    FreeRtos::delay_ms(1000);

    loop {
        match dht11::Reading::read(&mut delay::Ets, &mut sensor) {
            Ok(res) => info!(
                "temperature:{},humidity:{}",res.temperature,res.relative_humidity
            ),
            Err(_) => error!("no result"),
        }
        FreeRtos::delay_ms(3000);
    }

}
