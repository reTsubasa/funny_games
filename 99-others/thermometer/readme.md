# 温湿度计
## 部件
- esp32 c3 devkit m1
- dht11
- 线材

## 能干什么
通过DHT11 采集温度和湿度数据，并通过MQTT发送到ThingsCloud

算是wifi，mqtt，dht11 章节综合的实践。

simple, but funny

## 怎么跑
你需要在项目根目录下建立一个文件`cfg.toml`

```
[thermometer]
wifi_ssid = ""  #wifi name
wifi_psk = ""   #wifi pass


#mqtt args
mqtt_user = "xxxxxx"                      #mqtt user name
mqtt_pass = "xxxxxx"                            #mqtt user pass
mqtt_host = "xxxxx"     #mqtt endpoint
mqtt_clientid = "xxxxx"                          #mqtt client id
mqtt_topic = "attributes"                           #mqtt publish message topic
mqtt_push_topic = "command/send/+"                  #mqtt subscrib message topic

```

在项目根目录下执行`cargo run`, all things should ok.

## 已知问题
1. 比较多的error没有得到很好的处理，导致运行可靠性不高，容易panic
