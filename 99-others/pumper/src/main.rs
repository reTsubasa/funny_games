use core::str;
use std::time::{Duration, SystemTime};

use anyhow::{Result,Error};
use dht_sensor::{dht11, DhtReading};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::{adc, delay, gpio};
use esp_idf_svc::hal::adc::attenuation::DB_11;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::delay::{Delay, FreeRtos};
use esp_idf_svc::hal::gpio::{AnyIOPin, Gpio0, InputOutput, Level, PinDriver, Pins};
use esp_idf_svc::hal::prelude::Peripherals;
use esp_idf_svc::mqtt;
use esp_idf_svc::mqtt::client::QoS::AtMostOnce;
use esp_idf_svc::mqtt::client::{EspMqttClient, EventPayload, MqttProtocolVersion};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{payload_transfer_func, EspError};
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize,Debug)]
struct MqttMsg{
    solid_humidity:Option<u32>,
    relay:Option<bool>,
    amount_total:Option<u32>,
    environment_temperature:Option<u32>,
    environment_humidity:Option<u32>,
}
impl MqttMsg {
    fn new()->Self {
        Self{
            solid_humidity:None,
            relay:None,
            amount_total:None,
            environment_humidity:None,
            environment_temperature:None,
        }
    }
}


#[derive(Serialize, Deserialize,Debug)]
struct SolidHumidity {
    solid_humidity: u32,
}
#[derive(Serialize, Deserialize)]
struct PumperStatus {
    relay: bool,
}
impl PumperStatus {
    fn new(status: bool) -> Self {
        return Self { relay: status };
    }
}

struct PumperDriver<'a>{
    pin_drvier:PinDriver<'a, AnyIOPin, InputOutput>,
}


#[derive(Serialize, Deserialize)]
struct WateringAmount {
    amount_total: u32,
}

#[derive(Serialize, Deserialize)]
enum Instruct {
    Volumn(u32),
}

#[derive(Serialize, Deserialize)]
struct CloudCommand {
    method: String,
    params: Instruct,
    id: u32,
}

// pumper flow, as ”X ml/min“ usually can be found at motors
const PUMPER_FLOW: u32 = 50;

// range of Plant Moisture Meter in water & air
// as the max & min value can read from adcpin

// only fit for "Capacltlve Soll Molsture Sensor v2.0"
const MOISTURE_IN_WATER: u16 = 1450;
const MOISTURE_IN_AIR: u16 = 2837;

// sample time
// as ms
// default 30s( 30*1000 )
const LOOP_INTERVAL: u32 = 15 * 1000;

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
    mqtt_subscribe_topic: &'static str,
    #[default("")]
    pumper_volume: &'static str,
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let peripherals = Peripherals::take()?;

    // Hardware Setup
    // wifi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?,
        sysloop.clone(),
    )?;
    // relay
    // control the pump suck the water
    // use pin: gpio9
    // set low to stop & high to start,pump should keep stop as default
    let mut relay_pin: PinDriver<'_, esp_idf_svc::hal::gpio::Gpio9, esp_idf_svc::hal::gpio::InputOutput> = PinDriver::input_output(peripherals.pins.gpio9)?;
    relay_pin.set_low()?;

    // Plant Moisture Meter
    // One-shot ADC get the sample data from adc
    // use pin: gpio0
    // example https://github.com/esp-rs/esp-idf-hal/blob/master/examples/adc.rs
    // let config = AdcContConfig::default();
    // let adc_1_channel_0 = Attenuated::db11(peripherals.pins.gpio0);
    // let mut adc = AdcContDriver::new(peripherals.adc1,&config,adc_1_channel_0);
    let adc_1_channel_0: AdcDriver<'_, adc::ADC1> = AdcDriver::new(peripherals.adc1)?;
    let config = AdcChannelConfig {
        attenuation: DB_11,
        calibration: true,
        ..Default::default()
    };
    let mut adc: AdcChannelDriver<'_, Gpio0, &AdcDriver<'_, esp_idf_svc::hal::adc::ADC1>> =
        AdcChannelDriver::new(&adc_1_channel_0, peripherals.pins.gpio0, &config)?;

    // dht11
    let pin = peripherals.pins.gpio3;
    let mut dht_sensor = gpio::PinDriver::input_output(pin)?;
    dht_sensor.set_high().ok();
    FreeRtos::delay_ms(1000);


    
    // Process Init
    let app_config = CONFIG;
    // connect wifi
    while let Err(_) = wifi_connect(&mut wifi) {
        wifi_connect(&mut wifi)?;
    }

    // init mqtt client
    let mut client = mqtt_client_connect()?;

    // delays
    

    // loop
    loop {
        info!("start loop at:{:?}",SystemTime::now());
        // check wifi status
        wifi_health_checker(&mut wifi);

        // init mqtt msg struct
        let mut mqtt_msg = MqttMsg::new();


        // may use a struct hold all device handler,like a tree
        // may much much better
        // may do better at next version 

        // get relay status
        match relay_pin.get_level() {
            Level::Low => mqtt_msg.relay = Some(false),
            Level::High => mqtt_msg.relay = Some(true),
        }
        // get dht11 
        match dht11::Reading::read(&mut delay::Ets , &mut dht_sensor) {
            Ok(res) => {
                mqtt_msg.environment_temperature = Some(res.temperature as u32);
                mqtt_msg.environment_humidity = Some(res.relative_humidity as u32);
            },
            Err(e) => {
                error!("dht11 error:{:?}",e);
                continue;
            },
        }

        // read adc
        // should do adc adjust,make moisture into 2 stage, low value enable pumper water
        // and high value do next check
        //
        // filter： do read value 10 times in 10 secs
        // then cal total sum of 10 times，and sub max&min value，
        // todo! make pumper threshold tobe a var

        let mut moistures = Vec::new();

        for mut _i in 0..10 {
            match adc_1_channel_0.read(&mut adc) {
                Ok(val) => {
                    if val < MOISTURE_IN_WATER || val > MOISTURE_IN_AIR {
                        error!("moisture sensor error:{}",val);
                        // restart();
                    }
                    moistures.push(val);
                },
                Err(e) => error!("read adc error:{}",e),
            }
            
            FreeRtos::delay_ms(1000);
            _i += 1;
        }
        let humidity:u32;
        if moistures.len() == 10 {
            let min_value = *moistures.iter().min().unwrap_or(&MOISTURE_IN_WATER);
            let max_value = *moistures.iter().max().unwrap_or(&MOISTURE_IN_AIR);
            let moisture = (moistures.iter().sum::<u16>() - min_value - max_value) / 8;
            humidity = convert_moisture_to_humidity_u16(moisture);

        }else {
            error!("read moisture sensor 10 times");
            continue;
        }
        
        info!("humidity:{}",humidity);
        mqtt_msg.solid_humidity = Some(humidity);

        match mqtt_send_msg(&mut client,&mut mqtt_msg){
            Ok(_) => {
                info!("send step 1");
            },
            Err(e) => error!("mqtt client error:{}",e),
        }

        // adjust moistures
        // commet for real run
        // let value = adc_1_channel_0.read(&mut adc)?;
        // info!("sample moistures: {:?}", value);

        if  mqtt_msg.environment_temperature != None &&  mqtt_msg.environment_temperature < Some(2){
            continue;
        }

        // commet "match xxx" code if you adjust Plant Moisture Meter threshold
        match humidity {
            0..30 => {
                
                // run pumper time
                let volume = app_config.pumper_volume.parse::<u32>()?;
                let time = convert_volume_to_pumperworking_time_ms(volume
                );
                info!(
                    "pump starting!\nwater: {}ml, working time: {}ms",
                    app_config.pumper_volume, time
                );

                relay_pin.set_high()?;
                mqtt_msg.relay = Some(true);

                // mqtt publish gap
                FreeRtos::delay_ms(5000);
                match mqtt_send_msg(&mut client,&mut mqtt_msg){
                    Ok(_) => {
                        info!("send step 2");
                    },
                    Err(e) => error!("mqtt client error:{}",e),
                };

                FreeRtos::delay_ms(time -5000);

                while let Level::High = relay_pin.get_level(){
                    relay_pin.set_low().ok();
                    FreeRtos::delay_ms(100);
                }
                info!("pump stopped!");

                mqtt_msg.relay = Some(false);
                mqtt_msg.amount_total = Some(volume);

                match mqtt_send_msg(&mut client,&mut mqtt_msg){
                    Ok(_) => {
                        info!("send step 3");
                    },
                    Err(e) => error!("mqtt client error:{}",e),
                }
                

            }
            30.. => {
                info!("skip run pumper");
            }
        }

        // loop interval
        FreeRtos::delay_ms(LOOP_INTERVAL);
    }
}

fn wifi_connect(wifi: &mut BlockingWifi<EspWifi>) -> Result<(), EspError> {
    let app_config = CONFIG;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: app_config.wifi_ssid.try_into().unwrap(),
        password: app_config.wifi_psk.try_into().unwrap(),
        ..Default::default()
    }))?;

    match wifi.start() {
        Ok(_) => info!("Wifi started"),
        Err(e) => {
            error!("wifi start error:{}", e);
            return Err(e);
        }
    }
    match wifi.connect() {
        Ok(_) => info!("Wifi connected"),
        Err(e) => {
            error!("wifi connect error:{}", e);
            return Err(e);
        }
    }
    match wifi.wait_netif_up() {
        Ok(_) => info!("Wifi netif up"),
        Err(e) => {
            error!("wifi netif up error:{}", e);
            return Err(e);
        }
    }

    Ok(())
}

// wifi healthy check
// if status down re-connect
fn wifi_health_checker(wifi:&mut BlockingWifi<EspWifi>) {
    if !wifi_is_ok(wifi) {
        while let Some(_true) =  wifi.is_started().ok() {
            let _ = wifi.stop();
        };

        while let Err(_) = wifi_connect(wifi) {
            let _ = wifi_connect(wifi);
        }
    }
    info!("wifi reconnected!!");
}

// check if wifi status is ok
fn wifi_is_ok(wifi:&mut BlockingWifi<EspWifi>) -> bool{
    match wifi.is_up() {
        Ok(status) => {
            return status
        },
        Err(_) => false,
    }
}

fn mqtt_client_connect() -> Result<EspMqttClient<'static>> {
    // mqtt client
    let app_config = CONFIG;

    let mut client: EspMqttClient = EspMqttClient::new_cb(
        app_config.mqtt_host,
        &mqtt::client::MqttClientConfiguration {
            client_id: Some(app_config.mqtt_clientid),
            username: Some(app_config.mqtt_user),
            password: Some(&app_config.mqtt_pass),
            protocol_version: Some(MqttProtocolVersion::V3_1_1),
            network_timeout: Duration::from_secs(5),

            ..Default::default()
        },
        move |message_event| match message_event.payload() {
            // EventPayload::Error(e) => error!("MQTT error {:?}", e),
            // e => warn!("MQTT event {:?}", e),
            EventPayload::Received {
                id: _,
                topic,
                data,
                details: _,
            } => {
                info!("Received from MQTT topic:{:?}", topic.unwrap_or_default());
                received_message(data);
            }
            EventPayload::Error(e) => error!("MQTT error {:?}", e),
            e => warn!("MQTT event {:?}", e),
        },
    )?;


    let mut i: u32 = 0;
    loop {
        match client.subscribe(app_config.mqtt_subscribe_topic, AtMostOnce) {
            Ok(_) => {
                info!("Subscribed to topic:{}", app_config.mqtt_subscribe_topic);
                break;
            }
            Err(e) => {
                error!("Subscribed error:{} at {} times", e, i);
                i += 1;
            }
        }
        FreeRtos::delay_ms(1000);
    }

    Ok(client)
}

fn mqtt_send_msg(client:&mut EspMqttClient<'static>,mqtt_msg:&mut MqttMsg)->Result<(),Error>{
    let app_config = CONFIG;
    match serde_json::to_string(&mqtt_msg){
        Ok(payload) => {
            match client.enqueue(
                &app_config.mqtt_topic,
                AtMostOnce,
                false,
                payload.as_bytes(),
            ){
                Ok(_) => {
                    info!("send mqtt msg:{}",payload);
                    return Ok(());
                },
                Err(e) => {
                    error!("mqtt client error:{}",e);
                    return Err(e.into());
                },
            }
        },
        Err(e) => {
            error!("Serialize msg error:{}",e);
            return Err(e.into());
        },
    }
    
}

fn convert_moisture_to_humidity_u16(moisture: u16) -> u32 {
    let a = (MOISTURE_IN_AIR as u32  - moisture as u32 ) *100  / MOISTURE_IN_WATER as u32;
    return a;
}

// deal commands recieved from cloud
fn received_message(data: &[u8]) {
    match str::from_utf8(data) {
        Ok(res) => match serde_json::from_str::<CloudCommand>(res) {
            Ok(command) => match command.params {
                Instruct::Volumn(val) => {
                    info!("receive cloud command pumper water: {}ml", val);
                }
            },
            Err(e) => {
                error!("Phase Cloud Command Json failed:{}", e);
            }
        },
        Err(_) => {
            error!("Phase Cloud Command Failed");
        }
    }
}

// volume:water to pump,as ml
fn convert_volume_to_pumperworking_time_ms(volume: u32) -> u32 {
    return volume * 60 * 1000 / PUMPER_FLOW;
}
