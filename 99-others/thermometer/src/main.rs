use anyhow::Result;
use std::result::Result::Ok;
use dht_sensor::{dht11::{self, Reading}, DhtReading};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        delay::{self, FreeRtos},
        gpio::PinDriver,
        prelude::Peripherals,
    },
    mqtt::{
        self,
        client::{EspMqttClient, EventPayload, MqttProtocolVersion, QoS},
    },
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::{error, info, warn};
use serde::ser::{Serialize, SerializeStruct, Serializer};

#[toml_cfg::toml_config]
pub struct Config {
    #[default("localhost")]
    mqtt_host: &'static str,
    #[default("")]
    mqtt_user: &'static str,
    #[default("")]
    mqtt_pass: &'static str,
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
    #[default("")]
    mqtt_clientid: &'static str,
    #[default("")]
    mqtt_topic: &'static str,
    #[default("")]
    mqtt_push_topic: &'static str,
}

struct MyReading {
    reading: Reading,
}

impl Serialize for MyReading {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("temperature", 2)?;
        s.serialize_field("temperature", &self.reading.temperature)?;
        s.serialize_field("humidity", &self.reading.relative_humidity)?;
        s.end()
    }
}

//
fn wifi_connect(wifi: &mut BlockingWifi<EspWifi>) -> Result<(),anyhow::Error> {
    let app_config = CONFIG;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: app_config.wifi_ssid.try_into().unwrap(),
        password: app_config.wifi_psk.try_into().unwrap(),
        ..Default::default()
    }))?;

    while wifi.is_connected()? == false {
        wifi.start()?;
        info!("Wifi started");

        wifi.connect()?;
        info!("Wifi connected");

        wifi.wait_netif_up()?;
        info!("Wifi netif up");
    }

    Ok(())
}

fn mqtt_client_init() -> Result<EspMqttClient<'static>> {
    // mqtt client
    let app_config = CONFIG;
    // info!("connect args:{}",app_config.mqtt_host);
    // let mqtt_config = mqtt::client::MqttClientConfiguration::default();
    let client: EspMqttClient<'static> = EspMqttClient::new_cb(
        app_config.mqtt_host,
        &mqtt::client::MqttClientConfiguration {
            client_id: Some(app_config.mqtt_clientid),
            username: Some(app_config.mqtt_user),
            password: Some(&app_config.mqtt_pass),
            protocol_version: Some(MqttProtocolVersion::V3_1_1),
            ..Default::default()
        },
        move |message_event| match message_event.payload() {
            EventPayload::Error(e) => error!("MQTT error {:?}", e),
            e => warn!("MQTT event {:?}", e),
        },
    )?;

    Ok(client)
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let peripheral = Peripherals::take()?;

    // Hardware Setup
    // wifi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripheral.modem, sysloop.clone(), Some(nvs))?,
        sysloop.clone(),
    )?;
    // dht11
    let mut dht11_pin = PinDriver::input_output(peripheral.pins.gpio3)?;
    dht11_pin.set_high()?;

    // Start Process
    // connect wifi
    while  let Err(_) = wifi_connect(&mut wifi) {
        wifi_connect(&mut wifi)?;
    }
   
    // init mqtt client
    let app_config = CONFIG;
    let mut client = mqtt_client_init()?;

    info!("start loop!!");
    
    loop {
        // fetch dht11 data & send to MQTT server
        match dht11::Reading::read(&mut delay::Ets, &mut dht11_pin){
            Ok(res) => {
                let myres = MyReading { reading: res };
                let payload = serde_json::to_string(&myres)?;
                client.publish(
                    app_config.mqtt_topic,
                    QoS::AtMostOnce,
                    false,
                    payload.as_bytes(),
                )?;
            },
            Err(e)=> {
                error!("Reading DHT11 Data ERROR:{:?}",e);
            },
        };

        


        FreeRtos::delay_ms(1000 *10);
    }
}
