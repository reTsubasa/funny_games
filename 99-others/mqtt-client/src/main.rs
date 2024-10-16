use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::{delay::{Delay, FreeRtos}, prelude::Peripherals}, mqtt::{self, client::{EspMqttClient, EventPayload, MqttProtocolVersion, QoS}}, nvs::EspDefaultNvsPartition, wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi}
};
use log::{error, info, warn};

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
}

fn wifi() -> Result<BlockingWifi<EspWifi<'static>>> {
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let peripherals = Peripherals::take()?;

    // get wifi connect info
    let app_config = CONFIG;
    info!(
        "wifi config info: ssid:{},pass:{}",
        app_config.wifi_ssid, app_config.wifi_psk
    );

    // init wifi driver
    let mut wifi_driver = EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?;
    let mut wifi = BlockingWifi::wrap(wifi_driver, sysloop.clone())?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: app_config.wifi_ssid.try_into().unwrap(),
        password: app_config.wifi_psk.try_into().unwrap(),
        ..Default::default()
    }))?;

    wifi.start()?;
    info!("wifi start");

    wifi.connect()?;
    info!("wifi connected");

    wifi.wait_netif_up()?;

    Ok(wifi)
}


fn mqtt_client_init()->Result<EspMqttClient<'static>>{
    // mqtt client
    let app_config = CONFIG;
    // let mqtt_config = mqtt::client::MqttClientConfiguration::default();
    let mut client: EspMqttClient<'static>= EspMqttClient::new_cb(
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
    return Ok(client)
}
fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    // connect wifi
    let _wifi = wifi()?;

    // mqtt client
    let app_config = CONFIG;
    let mut client = mqtt_client_init()?;
    
    loop {
        // sample data！！
        let payload =  r#"
        {"temperature": 34.2,"percent": 70}"#;
        client.publish(app_config.mqtt_topic, QoS::AtMostOnce, false, payload.as_bytes())?;
        FreeRtos::delay_ms(1000*60);
    }
    Ok(())
}
