use anyhow::Result;
use log::info;
use esp_idf_svc::wifi::{self};
use esp_idf_svc::Peripherals::prelude::*;

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
}

fn wifi()->Result<()>{
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let peripherals = Peripherals::take()?;
    
    // get wifi connect info
    let app_config = CONFIG;
    info!("ssid:{},pass:{}",app_config.wifi_ssid,app_config.wifi_psk);

    // init wifi driver
    let wifi_driver = EspWifi::new();

}