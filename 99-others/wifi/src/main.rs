use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::Peripherals;
use esp_idf_svc::ipv4::Ipv4Addr;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::ping;
use esp_idf_svc::wifi::{self, ClientConfiguration};
use log::info;

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

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take()?;

    // let _wifi = wifi_create(&sys_loop, &nvs).unwrap();
    let peripherals = Peripherals::take()?;
    let app_config = CONFIG;
    info!("ssid:{}", app_config.wifi_ssid);

    let wifi_driver = wifi::EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs.clone()))?;
    let mut wifi = wifi::BlockingWifi::wrap(wifi_driver, sys_loop.clone())?;
    wifi.set_configuration(&wifi::Configuration::Client(ClientConfiguration {
        ssid: app_config.wifi_ssid.try_into().unwrap(),
        password: app_config.wifi_psk.try_into().unwrap(),
        ..Default::default()
    }))?;
    wifi.start()?;
    info!("wifi start");

    if let Ok(_) = wifi.connect(){
        info!("wifi connected");
    }else {
        wifi.connect()?;
    } ;
    

    wifi.wait_netif_up()?;

    loop {
        let mut espping = ping::EspPing::new(0_u32);
        let mut res = espping.ping(
            Ipv4Addr::new(10,10,13,254),
            &ping::Configuration::default(),
        )?;
        info!("{:?}", res);
    }
}
